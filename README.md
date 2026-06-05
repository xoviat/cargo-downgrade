# cargo-downgrade
```
Usage: downgrade [OPTIONS] <--date <DATE>|--git> [CARGO_LOCK] <COMMAND>

Commands:
  all   Downgrade all crate names of transitive dependencies in Cargo.lock file up to `dependency_level`
  this  Downgrade a list of specific crates
  help  Print this message or the help of the given subcommand(s)

Arguments:
  [CARGO_LOCK]  Path to the Cargo.lock file

Options:
  -d, --date <DATE>  Date to which the dependencies should be downgraded. In RFC 2822 format, e.g. "22 Feb 2021 23:16:09 GMT"
      --git          Get the date from git
      --run          Actually run the downgrade
  -h, --help         Print help
```