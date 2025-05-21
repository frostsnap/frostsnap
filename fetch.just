fetch SOURCE DEST:
  #!/bin/sh
  lockfile="justfetch.lock"
  url={{SOURCE}}
  dest={{DEST}}

  ends_with() {
    awk -v suf="$1" 'substr($0, length($0)-length(suf)+1) == suf { print }'
  }
  check_shasum() {
     echo "$1" | shasum -a 256 -c --status;
  }

  # Ensure destination directory exists
  mkdir -p "$(dirname "$dest")"

  line=""
  if test -e "$lockfile"; then
    line=$(grep -v '^#' < "$lockfile" | ends_with " $dest" )
  fi

  if ! test -e "$dest" || ! test "$line"; then
    echo "Downloading ‘$dest’" >&2;
    curl -L -S  "$url" -o "$dest"
  fi

  if test "$line"; then
    if ! check_shasum "$line"; then
      echo "⚠ Hash mismatch:\nexpected: $line\ngot     :" $(shasum -a 256 "$dest") >&2
      mv "$dest" "${dest}.dangerous"
      echo "⚠ moved '$dest' to '${dest}.dangerous'" >&2
      exit 1
    fi
  else
    shasum -a 256 "$dest" >> "$lockfile"
    echo "# source: $url" >> "$lockfile"
    echo "Recorded $dest in $lockfile" >&2
  fi
