# runtime: notes

Owner: `runtime` (see the roster in `docs/PARALLEL.md`).

## Done

- **Spec run speed.** The shim now sends every binary call to one
  long-lived `oxcop --server` per rspec process (`crates/oxcop/src/server.rs`,
  `Oxcop::Spec::Server` in `spec/oxcop/binary.rb`). Spawning from the spec
  process was the cost: with all ~600 spec files loaded its heap is large,
  and each `Open3.capture3` took ~25 ms (the binary itself runs in ~3 ms).
  Full `bundle exec rspec`: 46 s before, 6.4 s after (919 examples).
  - Same semantics as before: requests carry the exact argv (so
    `--raw-config`, `--persist-cops`, `--autocorrect-once` are unchanged),
    the cwd, and stdin bytes (length-prefixed, so invalid UTF-8 is fine).
    Each request runs `cli::run_with` with buffered stdout/stderr; a panic
    becomes that request's stderr and exit status 101.
  - Falls back to a fresh process when `ENV` differs from the server's
    (the binary reads `HOME`, `XDG_CONFIG_HOME`, ...), or when the server
    dies (it is restarted on the next call). `OXCOP_NO_SERVER=1` always
    spawns.
  - Output written deep inside a run goes through `cli::stderr()` (a
    thread-local sink), not `eprintln!`. **Other agents:** don't
    `eprintln!` from cops; report errors through the investigation's
    `errors`, or write to `crate::cli::stderr()`.

## Left

- Plugin default configs via the launcher; compare inside ../code's bundle.
- ERB in `.rubocop.yml`, `inherit_mode`, `inherit_gem`.
- Directive cops, pending-cops warning, CLI options, result cache, rayon.
- Parallel spec files (parallel_tests), once the suite grows enough that
  per-example Ruby time dominates.

## Gaps / blockers

None yet.
