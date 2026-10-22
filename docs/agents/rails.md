# rails: rubocop-rails cops

Owner: `rails` (see the roster in `docs/PARALLEL.md`). Ports from
rubocop-rails 2.37.0 (`/Users/teddy/_startup/rubocop-rs/rubocop-rails`).

## Status

Done: all three Round 3 cops are landed, each with its whole vendored spec
passing through the binary.

| Cop | Source | Spec examples |
| --- | --- | --- |
| Rails/TimeZoneAssignment | `cop/rails/time_zone_assignment.rs` | 3, 0 failures |
| Rails/HttpStatusNameConsistency | `cop/rails/http_status_name_consistency.rs` | 22, 0 failures |
| Rails/TimeZone | `cop/rails/time_zone.rs` | 96, 0 failures |

No principled spec failures; nothing pending.

Rails/TimeZone was also checked against real RuboCop + rubocop-rails on a
scratch file of edge cases (`::Time.now`, `Time::now`, `Time.at(-1234)`,
`Time.at(1i)`, dstr/xstr arguments, `localtime` with and without an
offset, chained `in_time_zone`, `&.`, adjacent string literals, 7-argument
`Time.new`), both styles, offenses and `-A` output: identical.

## Shared files touched (both landed with the full gate)

- `crates/oxcop/src/config_loader.rs`: with `--raw-config`, a plugin now
  counts as loaded when the configuration has its department key **or any
  of its cops' keys** (`Rails/...`). rubocop-rails' `:config` context dumps
  only `AllCops` and the cop under test, with no `Rails` key, so before this
  no Rails cop ran in its spec. rubocop-rspec specs were already covered
  (their shared context adds an `RSpec` key).
- `spec/oxcop/cop_helper.rb`, `spec/oxcop/binary.rb`: for a cop that
  `requires_gem` (`cop.class.gem_requirements` not empty), the shim sends
  the `:config` context's stubbed `gem_versions_in_target` (with versions,
  plus `path_sourced_gems_in_target`) through the binary's existing
  `--target-gems` flag. Before, `--target-gems` was only sent with
  `--project-index-sources`, so `let(:gem_versions)` never reached the
  binary. The Rust side (`Cop::gem_requirements`, `team.rs`'s filter,
  `read_target_gems`'s `versions`) already existed and needed no change.

## Notes for whoever continues

- `attach_timezone_specifier?` relies on `respond_to?(:value)`, which in
  rubocop-ast holds only for literal node classes (`str`, `xstr`, `dstr`,
  `sym`, numeric). `time_zone.rs`'s `literal_value` reproduces
  `value.to_s` for those, including the quirks (an `xstr`'s value is its
  first part node, whose `to_s` ends in `)`; an `int` like `-1234` matches
  the `[+-]dd:?dd` offset; a `Complex` ends in `i`).
- The gem-versions shim only applies to cops with `requires_gem`. A future
  Rails cop that reads `target_rails_version` without `TargetRailsVersion`
  set will need the stubbed `railties` version too; widening the condition
  in `cop_helper.rb#target_gems` (e.g. to every plugin cop) is the place.
- `mixin/target_rails_version.rb` (`minimum_target_rails_version`) is not
  ported; none of these three cops uses it.

## Decisions for the user

None.
