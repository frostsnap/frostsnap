use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use object::{Object, ObjectSymbol};
use rustc_demangle::demangle;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

const TARGET: &str = "riscv32imc-unknown-none-elf";

struct TypeInfo {
    name: String,
    size: u64,
    fields: Vec<FieldInfo>,
}

struct FieldInfo {
    name: String,
    size: u64,
    kind: FieldKind,
}

enum FieldKind {
    Field,
    Padding,
    Discriminant,
    Variant,
}

struct StackSymbols {
    stack_start: u64,
    stack_end: u64,
    available: u64,
}

#[derive(Parser)]
#[command(about = "Analyze stack and type sizes of the device firmware")]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,

    /// Device board binary name
    #[arg(global = true, long, default_value = "frontier")]
    board: String,
}

#[derive(Subcommand)]
enum Cmd {
    /// Analyze per-function stack frame sizes (default)
    Stacks {
        /// Show top N functions
        #[arg(long, default_value_t = 30)]
        top: usize,

        /// Skip building, just analyze existing binary
        #[arg(long)]
        no_build: bool,

        /// Filter functions matching this substring (case-insensitive)
        #[arg(long)]
        filter: Option<String>,

        /// Fail (exit code 1) if the largest frame exceeds this percentage of available stack
        #[arg(long)]
        max_pct: Option<f64>,
    },
    /// Show type sizes (requires nightly rebuild with -Zprint-type-sizes)
    Types {
        /// Show top N types
        #[arg(long, default_value_t = 30)]
        top: usize,

        /// Filter types matching this substring (case-insensitive)
        #[arg(long)]
        filter: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command.unwrap_or(Cmd::Stacks {
        top: 30,
        no_build: false,
        filter: None,
        max_pct: None,
    }) {
        Cmd::Stacks {
            top,
            no_build,
            filter,
            max_pct,
        } => cmd_stacks(&cli.board, top, no_build, filter, max_pct),
        Cmd::Types { top, filter } => cmd_types(&cli.board, top, filter),
    }
}

fn cmd_stacks(
    board: &str,
    top: usize,
    no_build: bool,
    filter: Option<String>,
    max_pct: Option<f64>,
) -> Result<()> {
    let elf_path = PathBuf::from(format!("target/{TARGET}/release/{board}"));

    if !no_build {
        build_firmware(board, &["-Z", "emit-stack-sizes"])?;
    }

    let elf_data = std::fs::read(&elf_path)
        .with_context(|| format!("failed to read ELF binary at {}", elf_path.display()))?;

    let functions = stack_sizes::analyze_executable(&elf_data)
        .context("failed to analyze ELF stack sizes (was it built with -Z emit-stack-sizes?)")?;

    let stack_info = read_stack_symbols(&elf_data)?;
    let filter_lower = filter.map(|f| f.to_lowercase());

    let mut entries: Vec<(String, u64)> = functions
        .defined
        .values()
        .filter_map(|func| {
            let stack = func.stack()?;
            let name = func.names().first().copied().unwrap_or("<unknown>");
            let demangled = format!("{:#}", demangle(name));
            if let Some(ref f) = filter_lower
                && !demangled.to_lowercase().contains(f.as_str())
            {
                return None;
            }
            Some((demangled, stack))
        })
        .collect();

    entries.sort_by_key(|e| Reverse(e.1));

    println!();
    if let Some(info) = &stack_info {
        println!(
            "Available stack: {:>6} bytes  (_stack_start={:#010X} _stack_end={:#010X})",
            info.available, info.stack_start, info.stack_end
        );
    }
    println!();

    println!(" {:>4}  {:>6}  {:>6}  Function", "Rank", "Stack", "%Avail");
    println!(" {:─>4}  {:─>6}  {:─>6}  {:─>60}", "", "", "", "");

    for (i, (name, stack)) in entries.iter().take(top).enumerate() {
        let pct = stack_info
            .as_ref()
            .map(|info| format!("{:>5.1}%", (*stack as f64 / info.available as f64) * 100.0))
            .unwrap_or_else(|| "   N/A".to_string());

        println!(
            " {:>4}  {:>6}  {}  {}",
            i + 1,
            stack,
            pct,
            truncate_name(name, 120)
        );
    }

    if let Some(info) = &stack_info {
        let total = entries.len();
        let large = entries.iter().filter(|e| e.1 > 2048).count();
        println!();
        println!("{total} functions total, {large} with stack frame > 2KB");
        if let Some((name, biggest)) = entries.first() {
            let pct = *biggest as f64 / info.available as f64 * 100.0;
            println!("Largest single frame: {biggest} bytes ({pct:.1}% of available stack)");

            if let Some(max) = max_pct
                && pct > max
            {
                bail!(
                    "FAILED: largest frame {biggest} ({pct:.1}%) exceeds \
                         --max-pct {max:.1}%: {name}"
                );
            }
        } else if max_pct.is_some() {
            bail!("--max-pct specified but no functions with stack size data found");
        }
    } else if max_pct.is_some() {
        bail!("cannot check --max-pct without stack symbols in the binary");
    }

    Ok(())
}

fn cmd_types(board: &str, top: usize, filter: Option<String>) -> Result<()> {
    // Full clean of riscv release artifacts required: -Zprint-type-sizes only
    // emits during compilation, and changing RUSTFLAGS changes the artifact
    // fingerprint so `cargo clean -p` won't touch the right artifacts.
    let target_dir = PathBuf::from(format!("target/{TARGET}/release"));
    if target_dir.exists() {
        eprintln!("Cleaning {target_dir:?} to force recompilation...");
        std::fs::remove_dir_all(&target_dir).context("failed to clean target dir")?;
    }

    let output = build_firmware_capture(board, &["-Z", "print-type-sizes"])?;

    // Show build progress from stderr
    let stderr_str = String::from_utf8_lossy(&output.stderr);
    for line in stderr_str.lines() {
        if !line.starts_with("print-type-size") {
            eprintln!("{line}");
        }
    }

    let filter_lower = filter.map(|f| f.to_lowercase());

    // Parse type lines and field lines from both stdout and stderr
    let combined = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);

    let mut types: Vec<TypeInfo> = Vec::new();
    let mut current: Option<TypeInfo> = None;

    for line in combined.lines() {
        if let Some(rest) = line.strip_prefix("print-type-size type: `") {
            if let Some(t) = current.take() {
                types.push(t);
            }
            // Parse: `TypeName`: SIZE bytes, alignment: ALIGN bytes
            if let Some(name_end) = rest.find("`: ") {
                let raw_name = &rest[..name_end];
                let after = &rest[name_end + 3..];
                // after looks like: "1456 bytes, alignment: 8 bytes"
                if let Some(size) = after
                    .split(',')
                    .next()
                    .and_then(|s| s.trim().strip_suffix(" bytes"))
                    .and_then(|n| n.trim().parse::<u64>().ok())
                {
                    let demangled = format!("{:#}", demangle(raw_name));
                    let should_include = filter_lower
                        .as_ref()
                        .is_none_or(|f| demangled.to_lowercase().contains(f.as_str()));
                    if should_include {
                        current = Some(TypeInfo {
                            name: demangled,
                            size,
                            fields: Vec::new(),
                        });
                    }
                }
            }
        } else if let Some(t) = current.as_mut() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("print-type-size") {
                let rest = rest.trim();
                if let Some(field_rest) = rest.strip_prefix("field `.") {
                    if let Some(name_end) = field_rest.find("`: ") {
                        let field_name = &field_rest[..name_end];
                        let size_str = &field_rest[name_end + 3..];
                        if let Some(size) = size_str
                            .strip_suffix(" bytes")
                            .and_then(|s| s.trim().parse::<u64>().ok())
                        {
                            t.fields.push(FieldInfo {
                                name: field_name.to_string(),
                                size,
                                kind: FieldKind::Field,
                            });
                        }
                    }
                } else if let Some(pad_rest) = rest.strip_prefix("end padding: ") {
                    if let Some(size) = pad_rest
                        .strip_suffix(" bytes")
                        .and_then(|s| s.trim().parse::<u64>().ok())
                    {
                        t.fields.push(FieldInfo {
                            name: "padding".to_string(),
                            size,
                            kind: FieldKind::Padding,
                        });
                    }
                } else if let Some(disc_rest) = rest.strip_prefix("discriminant: ") {
                    if let Some(size) = disc_rest
                        .strip_suffix(" bytes")
                        .and_then(|s| s.trim().parse::<u64>().ok())
                    {
                        t.fields.push(FieldInfo {
                            name: "discriminant".to_string(),
                            size,
                            kind: FieldKind::Discriminant,
                        });
                    }
                } else if let Some(variant_rest) = rest.strip_prefix("variant `")
                    && let Some(name_end) = variant_rest.find("`: ")
                {
                    let variant_name = &variant_rest[..name_end];
                    let size_str = &variant_rest[name_end + 3..];
                    if let Some(size) = size_str
                        .strip_suffix(" bytes")
                        .and_then(|s| s.trim().parse::<u64>().ok())
                    {
                        t.fields.push(FieldInfo {
                            name: variant_name.to_string(),
                            size,
                            kind: FieldKind::Variant,
                        });
                    }
                }
            }
        }
    }

    if let Some(t) = current.take() {
        types.push(t);
    }

    types.sort_by_key(|t| Reverse(t.size));

    println!();
    println!(" {:>4}  {:>6}  Type", "Rank", "Bytes");
    println!(" {:─>4}  {:─>6}  {:─>60}", "", "", "");

    for (i, t) in types.iter().take(top).enumerate() {
        println!(
            " {:>4}  {:>6}  {}",
            i + 1,
            t.size,
            truncate_name(&t.name, 120)
        );
        for field in &t.fields {
            let label = match field.kind {
                FieldKind::Field => format!(".{}", field.name),
                FieldKind::Padding => "padding".to_string(),
                FieldKind::Discriminant => "discriminant".to_string(),
                FieldKind::Variant => format!("variant {}", field.name),
            };
            println!("              {:>6}    {}", field.size, label);
        }
    }

    println!();
    println!("{} types analyzed", types.len());

    Ok(())
}

fn read_stack_symbols(elf_data: &[u8]) -> Result<Option<StackSymbols>> {
    let file = object::File::parse(elf_data).context("failed to parse ELF")?;

    let mut symbols: HashMap<&str, u64> = HashMap::new();
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            symbols.insert(name, sym.address());
        }
    }

    let stack_start = symbols.get("_stack_start").copied();
    let stack_end = symbols.get("_stack_end").copied();

    match (stack_start, stack_end) {
        (Some(start), Some(end)) if start > end => Ok(Some(StackSymbols {
            stack_start: start,
            stack_end: end,
            available: start - end,
        })),
        (Some(start), Some(end)) => {
            eprintln!(
                "warning: _stack_start ({start:#x}) <= _stack_end ({end:#x}), \
                 cannot compute stack size"
            );
            Ok(None)
        }
        _ => Ok(None),
    }
}

/// Truncate a demangled name to `max_len`, keeping the start (type path) and
/// end (function name) visible, eliding generic parameters in the middle.
fn truncate_name(name: &str, max_len: usize) -> String {
    if name.len() <= max_len {
        return name.to_string();
    }

    // Find the last `::` or `>` to identify the function/method name at the end
    let suffix_start = name
        .rfind("::")
        .map(|p| p + 2)
        .or_else(|| name.rfind('>').map(|p| p + 1));

    if let Some(suffix_start) = suffix_start {
        let suffix = &name[suffix_start..];
        // Budget: max_len - suffix - " .. "
        let prefix_budget = max_len.saturating_sub(suffix.len() + 4);
        if prefix_budget > 10 {
            // Cut prefix at a clean boundary
            let prefix_region = &name[..prefix_budget.min(name.len())];
            let cut = prefix_region
                .rfind('<')
                .or_else(|| prefix_region.rfind("::"))
                .unwrap_or(prefix_budget);
            if cut > 10 {
                return format!("{} .. {}", &name[..cut], suffix);
            }
        }
    }

    // Fallback: just truncate at a clean boundary
    let search = &name[..max_len];
    if let Some(pos) = search.rfind('<')
        && pos > 20
    {
        return name[..pos].to_string();
    }
    name[..max_len].to_string()
}

/// Build the device firmware with nightly and extra RUSTFLAGS, streaming output to stderr.
fn build_firmware(board: &str, extra_rustflags: &[&str]) -> Result<()> {
    let status = cargo_build_command(board, extra_rustflags)
        .status()
        .context("failed to invoke cargo")?;

    if !status.success() {
        bail!("cargo build failed");
    }

    eprintln!();
    Ok(())
}

/// Build the device firmware capturing stdout (for parsing compiler output like -Zprint-type-sizes).
fn build_firmware_capture(board: &str, extra_rustflags: &[&str]) -> Result<std::process::Output> {
    let output = cargo_build_command(board, extra_rustflags)
        .output()
        .context("failed to invoke cargo")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprint!("{stderr}");
        bail!("cargo build failed");
    }

    Ok(output)
}

fn cargo_build_command(board: &str, extra_rustflags: &[&str]) -> Command {
    let device_dir = PathBuf::from("device");

    let mut rustflags = std::env::var("RUSTFLAGS").unwrap_or_default();
    if !rustflags.contains("-Tlinkall.x") {
        if !rustflags.is_empty() {
            rustflags.push(' ');
        }
        rustflags.push_str("-C link-arg=-Tlinkall.x");
    }
    for flag in extra_rustflags {
        rustflags.push(' ');
        rustflags.push_str(flag);
    }

    eprintln!("Building {board}...");

    let mut cmd = Command::new("cargo");
    cmd.arg("+nightly")
        .arg("build")
        .arg("--release")
        .arg("--locked")
        .arg("--bin")
        .arg(board)
        .arg("-Z")
        .arg("build-std=alloc,core")
        .env("RUSTFLAGS", &rustflags)
        .env("CARGO_PROFILE_RELEASE_STRIP", "none")
        .current_dir(&device_dir);
    cmd
}
