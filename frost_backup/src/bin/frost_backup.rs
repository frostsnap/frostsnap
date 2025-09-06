use clap::{Parser, Subcommand};
use core::convert::TryInto;
use frost_backup::*;
use schnorr_fun::fun::prelude::*;
use std::fs;
use std::str::FromStr;

fn print_derivation_path_explanation() {
    eprintln!("🗺️  Derivation path explanation:");
    eprintln!("   The descriptor uses the path: /0/0/0/0/<0;1>/*");
    eprintln!("                                 │ │ │ │   │   └─ Address index (wildcard)");
    eprintln!("                                 │ │ │ │   └─ Keychain (0=external, 1=internal)");
    eprintln!("                                 │ │ │ └─ Account index (0=first account)");
    eprintln!("                                 │ │ └─ Account type (0=segwit v1/taproot)");
    eprintln!("                                 │ └─ App type (0=Bitcoin)");
    eprintln!("                                 └─ Root to master (Frostsnap convention)");
}

#[derive(Parser)]
#[command(name = "frost_backup")]
#[command(about = "BIP39-based backup scheme for Shamir secret shares")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Generate {
        #[arg(help = "Threshold of shares needed to reconstruct")]
        threshold: usize,

        #[arg(help = "Total number of shares to generate")]
        number_of_shares: usize,

        #[arg(help = "Secret as 32 bytes hex (optional - will generate random if not provided)")]
        secret: Option<Scalar>,

        #[arg(
            short = 'o',
            long = "output",
            help = "Output file for all shares (use '-' for stdout, omit to print all shares to stdout)"
        )]
        output: Option<String>,

        #[arg(
            short = 'y',
            long = "yes",
            help = "Skip confirmation prompt (not recommended for production use)"
        )]
        yes: bool,
    },
    Reconstruct {
        #[arg(
            help = "Share files to reconstruct from (use '-' for stdin with EOF (Ctrl+D) to finish)",
            required = true,
            num_args = 1..
        )]
        files: Vec<String>,

        #[arg(
            short = 't',
            long = "threshold",
            help = "Threshold of shares needed to reconstruct (optional - will try to discover if not provided)"
        )]
        threshold: Option<usize>,

        #[arg(
            help = "Network for the descriptor output",
            default_value = "main",
            short,
            long,
            value_parser = ["main", "test"]
        )]
        network: String,

        #[arg(
            long = "no-check-fingerprint",
            help = "Skip fingerprint checking (uses 0-bit fingerprint). Requires --threshold to be specified"
        )]
        no_check_fingerprint: bool,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Generate {
            threshold,
            number_of_shares,
            secret,
            output,
            yes,
        } => {
            if !yes {
                eprintln!("⚠️  WARNING: Local Key Generation Not Recommended");
                eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                eprintln!("📱 This command is intended primarily for testing and");
                eprintln!("   advanced users who understand the security implications.");
                eprintln!();
                eprintln!("🔒 Real keys used to protect funds should be generated using");
                eprintln!("   Frostsnap devices through a truly 'Distributed'-DKG process.");
                eprintln!();
                eprint!("Continue with local key generation? [y/N]: ");

                use std::io::{self, Write};
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                match input.trim().to_lowercase().as_str() {
                    "y" | "yes" => {
                        eprintln!("Proceeding with local generation...");
                    }
                    _ => {
                        eprintln!("Aborted. Use Frostsnap devices for secure key generation.");
                        return Ok(());
                    }
                }
            }

            // Validate parameters
            if threshold > number_of_shares {
                return Err("Threshold cannot be greater than number of shares".into());
            }
            if threshold == 0 {
                return Err("Threshold must be at least 1".into());
            }

            // Use provided secret or generate a random one
            let mut rng = rand::thread_rng();
            let secret_scalar = match secret {
                Some(s) => s,
                None => Scalar::random(&mut rng),
            };

            // Generate shares with randomness
            let (shares, shared_key) = ShareBackup::generate_shares(
                secret_scalar,
                threshold,
                number_of_shares,
                frost_backup::FINGERPRINT,
                &mut rng,
            );

            // Print metadata to stderr so stdout can be used for shares
            if secret.is_none() {
                eprintln!("🎲 Generated random secret:");
                eprintln!("   {}", secret_scalar);
            }

            eprintln!("🔑 Root Public key: {}", shared_key.public_key());

            // Generate taproot descriptor
            let descriptor = generate_descriptor(&secret_scalar, bitcoin::NetworkKind::Main);
            eprintln!("📜 Bitcoin descriptor:");
            eprintln!("   {}", descriptor);

            // Print derivation path explanation
            print_derivation_path_explanation();

            // Always show critical backup instructions
            eprintln!();
            eprintln!("⚠️  ⚠️  ⚠️  CRITICAL BACKUP INSTRUCTIONS ⚠️  ⚠️  ⚠️");
            eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            eprintln!("🔥 WRITE EACH SHARE DOWN IN A SEPARATE, SECURE PLACE");
            eprintln!("📍 INCLUDE THE SHARE NUMBER (e.g., #1, #2, #3)");
            eprintln!(
                "🔐 ANY {} OUT OF {} SHARES WILL RESTORE YOUR WALLET",
                threshold, number_of_shares
            );
            eprintln!(
                "💀 LOSING MORE THAN {} SHARES MEANS LOSING YOUR FUNDS FOREVER",
                number_of_shares - threshold
            );
            eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

            match output {
                Some(out) if out == "-" => {
                    // Output all shares to stdout
                    for share in &shares {
                        println!("{}", share);
                    }
                }
                Some(out) => {
                    // Check if file already exists
                    if std::path::Path::new(&out).exists() {
                        return Err(format!(
                            "File '{}' already exists. Please use a different filename or remove the existing file.",
                            out
                        ).into());
                    }

                    // Output all shares to file
                    let mut content = String::new();
                    for share in &shares {
                        content.push_str(&share.to_string());
                        content.push('\n');
                    }
                    fs::write(&out, content)?;
                    eprintln!();
                    eprintln!("💾 Saved all {} shares to {}", number_of_shares, out);
                }
                None => {
                    // Output all shares to stdout with formatting
                    for (i, share) in shares.iter().enumerate() {
                        eprintln!("════════════════════════════════════════════════");
                        eprintln!("SHARE #{} - STORE THIS SEPARATELY:", i + 1);
                        eprintln!("════════════════════════════════════════════════");
                        println!("{}", share);
                        eprintln!();
                    }
                }
            }

            eprintln!();
            eprintln!(
                "✅ Successfully generated {} shares with threshold {}",
                number_of_shares, threshold
            );
            eprintln!("🚨 REMINDER: Store each share in a different physical location!");
            eprintln!("   Examples: safety deposit boxes, trusted family members, secure safes");
        }

        Commands::Reconstruct {
            files,
            threshold,
            network,
            no_check_fingerprint,
        } => {
            // Validate that threshold is provided when using --no-check-fingerprint
            if no_check_fingerprint && threshold.is_none() {
                return Err("Threshold must be specified when using --no-check-fingerprint".into());
            }

            let mut shares = Vec::new();

            // Read shares from specified files
            for file in files {
                if file == "-" {
                    // Read multiple shares from stdin, one per line
                    use std::io::{self, BufRead, BufReader};
                    let stdin = io::stdin();
                    let reader = BufReader::new(stdin);

                    for line in reader.lines() {
                        let line = line?;
                        let line = line.trim();

                        // Skip empty lines
                        if line.is_empty() {
                            continue;
                        }

                        match ShareBackup::from_str(line) {
                            Ok(share) => {
                                let index_u32: u32 = share.index().try_into().unwrap();
                                eprintln!("📥 Loaded share #{} from stdin", index_u32);
                                shares.push(share);
                            }
                            Err(e) => {
                                return Err(
                                    format!("Failed to parse share from stdin: {}", e).into()
                                );
                            }
                        }
                    }
                } else {
                    // Read from file
                    let content = fs::read_to_string(&file)?;

                    match ShareBackup::from_str(content.trim()) {
                        Ok(share) => {
                            let index_u32: u32 = share.index().try_into().unwrap();
                            eprintln!("📁 Loaded share #{} from {}", index_u32, file);
                            shares.push(share);
                        }
                        Err(e) => {
                            return Err(
                                format!("Failed to parse share from {}: {}", file, e).into()
                            );
                        }
                    }
                }
            }

            // Reconstruct the secret
            let fingerprint = if no_check_fingerprint {
                eprintln!();
                eprintln!("⚠️  Warning: Skipping fingerprint check - accepting any valid {}-of-{} polynomial", 
                    threshold.unwrap(), shares.len());
                // Use 0-bit fingerprint (no checking)
                schnorr_fun::frost::Fingerprint::NONE
            } else {
                frost_backup::FINGERPRINT
            };

            let recovered = recovery::recover_secret_fuzzy(&shares, fingerprint, threshold)
                .ok_or("❌ Failed to find a valid subset of shares")?;

            eprintln!();
            eprintln!("🔓 Reconstructed secret:");
            eprintln!("   {}", recovered.secret);
            eprintln!("🔑 Public key: {}", recovered.shared_key.public_key());

            // Show which share indices were used
            let used_indices: Vec<String> = recovered
                .compatible_shares
                .iter()
                .map(|idx| format!("#{}", idx))
                .collect();
            eprintln!("📊 Compatible shares found: {}", used_indices.join(", "));

            // Check if the secret is zero (extremely unlikely but theoretically possible)
            match recovered.secret.non_zero() {
                Some(non_zero_secret) => {
                    // Generate taproot descriptor
                    let net = match network.as_str() {
                        "test" => bitcoin::NetworkKind::Test,
                        _ => bitcoin::NetworkKind::Main,
                    };
                    let descriptor = frost_backup::generate_descriptor(&non_zero_secret, net);

                    // Print the descriptor to stdout for piping
                    println!("{}", descriptor);

                    eprintln!("📜 Bitcoin descriptor:");
                    eprintln!("   {}", descriptor);

                    // Print derivation path explanation
                    print_derivation_path_explanation();
                }
                None => {
                    eprintln!();
                    eprintln!(
                        "⚠️  The recovered secret is zero - no valid descriptor can be generated."
                    );
                    return Ok(());
                }
            }
        }
    }

    Ok(())
}
