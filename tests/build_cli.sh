#!/usr/bin/env bash
# Regression coverage for the canonical Rust build command.
set -euo pipefail

repo_root=$(cd "$(dirname "$0")/.." && pwd -P)
# shellcheck source=../build.sh
. "$repo_root/build.sh"
_color_init

test_count=0

fail() {
  printf 'build CLI test failed: %s\n' "$1" >&2
  exit 1
}

assert_equal() {
  [ "$1" = "$2" ] || fail "expected [$2], got [$1]"
}

assert_contains() {
  case "$1" in *"$2"*) ;; *) fail "output missing [$2]: $1" ;; esac
}

assert_not_contains() {
  case "$1" in *"$2"*) fail "output unexpectedly contains [$2]: $1" ;; *) ;; esac
}

pass() { test_count=$((test_count + 1)); }

new_fixture() {
  mktemp -d "${TMPDIR:-/tmp}/govna-rust-build-test.XXXXXX"
}

test_utility_declaration_validation() {
  local fixture output rc
  fixture=$(new_fixture) || fail 'create declaration fixture'
  printf 'const PROGRAM_VERSION: &str = "1.2.3";\nfn value() -> &str { PROGRAM_VERSION }\n' >"$fixture/tool.rs"
  _read_utility_version tool "$fixture/tool.rs" || fail 'valid declaration rejected'
  assert_equal "$_utility_version_value" '1.2.3'

  printf 'const PROGRAM_VERSION: &str = "01.2.3";\n' >"$fixture/tool.rs"
  set +e
  output=$(_read_utility_version tool "$fixture/tool.rs" 2>&1)
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" 'malformed PROGRAM_VERSION'

  printf 'const PROGRAM_VERSION: &str = "1.2.3";\nconst PROGRAM_VERSION: &str = "1.2.4";\n' >"$fixture/tool.rs"
  set +e
  output=$(_read_utility_version tool "$fixture/tool.rs" 2>&1)
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" 'duplicate PROGRAM_VERSION'
  rm -rf -- "$fixture"
  pass
}

write_version_binary() { # $1=path $2=stdout $3=stderr $4=exit
  local path="$1" stdout="$2" stderr="$3" status="$4"
  {
    printf '%s\n' '#!/usr/bin/env bash'
    printf 'printf %s %s\n' "'%b'" "'$stdout'"
    if [ -n "$stderr" ]; then
      printf 'printf %s %s >&2\n' "'%b'" "'$stderr'"
    fi
    printf 'exit %s\n' "$status"
  } >"$path"
  chmod +x "$path"
}

test_compiled_version_output() {
  local fixture output rc
  fixture=$(new_fixture) || fail 'create output fixture'
  mkdir -p "$fixture/release"
  _cargo_target="$fixture"

  write_version_binary "$fixture/release/tool" 'tool 1.2.3\n' '' 0
  _validate_compiled_utility tool 1.2.3 >/dev/null || fail 'bare version rejected'
  write_version_binary "$fixture/release/tool" 'tool v1.2.3\n' '' 0
  _validate_compiled_utility tool 1.2.3 >/dev/null || fail 'v-prefixed version rejected'

  write_version_binary "$fixture/release/tool" 'tool version 1.2.3\n' '' 0
  set +e
  output=$(_validate_compiled_utility tool 1.2.3 2>&1)
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" 'expected exactly'

  write_version_binary "$fixture/release/tool" 'tool 1.2.3\n' 'diagnostic\n' 0
  set +e
  output=$(_validate_compiled_utility tool 1.2.3 2>&1)
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" 'wrote to stderr'
  rm -rf -- "$fixture"
  pass
}

test_manifest_path_mapping() {
  local fixture output rc
  fixture=$(new_fixture) || fail 'create manifest fixture'
  mkdir -p "$fixture/src/custom" "$fixture/tests"
  printf 'const PROGRAM_VERSION: &str = "2.0.0";\nfn main() {}\n' >"$fixture/src/custom/tool.rs"
  printf '#[test]\nfn tool_cli() {}\n' >"$fixture/tests/tool_cli.rs"
  {
    printf '[package]\nname = "fixture"\nversion = "1.0.0"\n'
    printf '[[bin]]\nname = "tool"\npath = "src/custom/tool.rs"\n'
  } >"$fixture/Cargo.toml"
  (
    cd "$fixture" || exit 1
    _load_bin_targets
    _validate_utility_declarations >/dev/null
    assert_equal "${_bin_targets[0]}" tool
    assert_equal "${_bin_paths[0]}" src/custom/tool.rs
  ) || fail 'declared path mapping failed'

  printf 'const PROGRAM_VERSION: &str = "9.9.9";\n' >"$fixture/src/orphan.rs"
  set +e
  output=$(cd "$fixture" && _load_bin_targets && _validate_utility_declarations 2>&1)
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" 'orphaned PROGRAM_VERSION declaration'
  rm -f "$fixture/src/orphan.rs"

  printf 'fn main() {}\n' >"$fixture/src/custom/tool.rs"
  set +e
  output=$(cd "$fixture" && _load_bin_targets && _validate_utility_declarations 2>&1)
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" 'missing PROGRAM_VERSION'
  rm -rf -- "$fixture"
  pass
}

test_install_reporting() {
  local fixture output report
  fixture=$(new_fixture) || fail 'create install fixture'
  mkdir -p "$fixture/bin"
  _cargo_target="$fixture"
  report="$fixture/install-output"
  _run_cargo_install tool 3.4.5 "$fixture/bin/tool" 0 \
    sh -c 'printf installed >"$1"' sh "$fixture/bin/tool" >"$report" ||
    fail 'install reporting failed'
  output=$(cat "$report")
  assert_contains "$output" 'Installing'
  assert_contains "$output" "$fixture/bin/tool"
  assert_contains "$output" 'v3.4.5'
  rm -rf -- "$fixture"
  pass
}

test_prep_no_build_rejection() {
  local output rc
  set +e
  output=$(
    _validate_release_inputs() { return 0; }
    _validate_git_state() { return 0; }
    _validate_canon_version_bump() { return 0; }
    _cargo_version_info() { printf '1.0.0\n'; }
    _govna_program_version_info() { printf '1.0.0\n'; }
    _ac_refs() { printf ''; }
    _matching_ac_files() { printf ''; }
    prep_run 0 1 0 v1.0.1 release 2>&1
  )
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" 'cannot use --no-build/-B'
  pass
}

new_git_fixture() {
  local fixture tree commit
  fixture=$(new_fixture) || return 1
  git -C "$fixture" init -q || return 1
  printf '*.ignored\n' >"$fixture/.gitignore"
  printf 'tracked\n' >"$fixture/tracked"
  printf 'target-one\n' >"$fixture/target-one"
  printf 'target-two\n' >"$fixture/target-two"
  ln -s target-one "$fixture/link"
  git -C "$fixture" add . || return 1
  tree=$(git -C "$fixture" write-tree) || return 1
  commit=$(printf 'fixture\n' | git -C "$fixture" \
    -c user.name=fixture -c user.email=fixture@example.invalid \
    commit-tree "$tree") || return 1
  git -C "$fixture" update-ref HEAD "$commit" || return 1
  printf '%s\n' "$fixture"
}

test_validation_token_tracks_git_visible_state() {
  local fixture first second tree commit parent
  fixture=$(new_git_fixture) || fail 'create token fixture'
  first=$(cd "$fixture" && _validation_token) || fail 'create initial token'
  second=$(cd "$fixture" && _validation_token) || fail 'repeat initial token'
  assert_equal "$second" "$first"
  case "$first" in v2:*:*:*) ;; *) fail "malformed token: $first" ;; esac

  printf 'changed\n' >"$fixture/tracked"
  second=$(cd "$fixture" && _validation_token)
  [ "$second" != "$first" ] || fail 'tracked content did not change token'
  printf 'tracked\n' >"$fixture/tracked"

  chmod +x "$fixture/tracked"
  second=$(cd "$fixture" && _validation_token)
  [ "$second" != "$first" ] || fail 'executable mode did not change token'
  chmod -x "$fixture/tracked"

  rm "$fixture/link"
  ln -s target-two "$fixture/link"
  second=$(cd "$fixture" && _validation_token)
  [ "$second" != "$first" ] || fail 'symlink target did not change token'
  rm "$fixture/link"
  ln -s target-one "$fixture/link"

  printf 'new\n' >"$fixture/untracked"
  second=$(cd "$fixture" && _validation_token)
  [ "$second" != "$first" ] || fail 'untracked content did not change token'
  rm "$fixture/untracked"

  printf 'ignored\n' >"$fixture/cache.ignored"
  second=$(cd "$fixture" && _validation_token)
  assert_equal "$second" "$first"
  printf 'internal\n' >"$fixture/.git/internal-change"
  second=$(cd "$fixture" && _validation_token)
  assert_equal "$second" "$first"

  tree=$(git -C "$fixture" write-tree)
  parent=${first#v2:}
  parent=${parent%%:*}
  commit=$(printf 'next\n' | git -C "$fixture" \
    -c user.name=fixture -c user.email=fixture@example.invalid \
    commit-tree "$tree" -p "$parent")
  git -C "$fixture" update-ref HEAD "$commit"
  second=$(cd "$fixture" && _validation_token)
  [ "$second" != "$first" ] || fail 'HEAD did not change token'
  rm -rf -- "$fixture"
  pass
}

test_baseline_validation_token_refresh() {
  local fixture scratch first refreshed second output rc first_reduced refreshed_reduced
  fixture=$(new_git_fixture) || fail 'create refresh fixture'
  scratch=$(mktemp "${TMPDIR:-/tmp}/canon-baseline.XXXXXX") || fail 'create scratch baseline'
  printf 'govna-canon-baseline-v1\ncanon_version = v1.0.0\n' >"$scratch"
  first=$(cd "$fixture" && _validation_token) || fail 'create prior token'
  mkdir -p "$fixture/govna"
  cp "$scratch" "$fixture/govna/canon-baseline.txt"
  refreshed=$(cd "$fixture" && refresh_validation_token_run "$scratch" "$first") ||
    fail 'refresh baseline-only token'
  [ "$refreshed" != "$first" ] || fail 'refresh did not update full token'
  first_reduced=${first##*:}
  refreshed_reduced=${refreshed##*:}
  assert_equal "$refreshed_reduced" "$first_reduced"
  second=$(cd "$fixture" && _validation_token)
  assert_equal "$second" "$refreshed"

  printf 'changed\n' >"$fixture/tracked"
  set +e
  output=$(cd "$fixture" && refresh_validation_token_run "$scratch" "$first" 2>&1)
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" 'changed beyond govna/canon-baseline.txt'
  printf 'tracked\n' >"$fixture/tracked"

  printf 'different\n' >"$fixture/govna/canon-baseline.txt"
  set +e
  output=$(cd "$fixture" && refresh_validation_token_run "$scratch" "$first" 2>&1)
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" 'differs from the scratch render'

  cp "$scratch" "$fixture/govna/canon-baseline.txt"
  cp "$scratch" "$fixture/internal-baseline"
  set +e
  output=$(cd "$fixture" && refresh_validation_token_run internal-baseline "$first" 2>&1)
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" 'must be outside the repository'
  rm "$fixture/internal-baseline"

  rm "$fixture/govna/canon-baseline.txt"
  set +e
  output=$(cd "$fixture" && refresh_validation_token_run "$scratch" "$first" 2>&1)
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" 'installed govna/canon-baseline.txt must be a regular file'
  cp "$scratch" "$fixture/govna/canon-baseline.txt"

  rm "$fixture/govna/canon-baseline.txt"
  second=$(cd "$fixture" && _validation_token)
  [ "$second" != "$refreshed" ] || fail 'baseline deletion did not stale refreshed token'
  cp "$scratch" "$fixture/govna/canon-baseline.txt"
  printf 'replacement\n' >"$scratch"
  cp "$scratch" "$fixture/govna/canon-baseline.txt"
  second=$(cd "$fixture" && _validation_token)
  [ "$second" != "$refreshed" ] || fail 'second baseline replacement did not stale token'

  set +e
  output=$(cd "$fixture" && refresh_validation_token_run "$scratch" 'v1:old:token' 2>&1)
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" 'malformed or unsupported'
  rm -rf -- "$fixture" "$scratch"
  pass
}

test_refresh_validation_token_cli_contract() {
  local output rc
  refresh_validation_token_run() { printf '%s|%s\n' "$1" "$2"; }
  output=$(refresh_validation_token_main -b /tmp/baseline -t token)
  assert_equal "$output" '/tmp/baseline|token'
  output=$(refresh_validation_token_main --baseline /tmp/baseline --token token)
  assert_equal "$output" '/tmp/baseline|token'
  set +e
  output=$(refresh_validation_token_main -b one -b two -t token 2>&1)
  rc=$?
  set -e
  assert_equal "$rc" 2
  assert_contains "$output" 'usage:'
  pass
}

test_shared_cargo_target_ownership() {
  local fixture shared output rc
  fixture=$(new_git_fixture) || fail 'create shared-target fixture'
  shared=$(mktemp -d "${TMPDIR:-/tmp}/govna-rust-target.XXXXXX") ||
    fail 'create shared target'
  (
    cd "$fixture" || exit 1
    GOVNA_PREP_CARGO_TARGET_DIR="$shared"
    _create_cargo_target
    assert_equal "$_cargo_target_owned" 0
    _cleanup_cargo_target
    [ -d "$shared" ]
  ) || fail 'borrowed target ownership failed'

  set +e
  output=$(cd "$fixture" && GOVNA_PREP_CARGO_TARGET_DIR=relative _create_cargo_target 2>&1)
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" 'must be an absolute path'
  rm -rf -- "$shared" "$fixture"
  pass
}

test_prep_evidence_routing() {
  local output
  output=$(
    _validate_release_inputs() { return 0; }
    _validate_git_state() { return 0; }
    _validate_canon_version_bump() { return 0; }
    _cargo_version_info() { printf '1.0.0\n'; }
    _ac_refs() { printf ''; }
    _matching_ac_files() { printf ''; }
    _govna_program_version_info() { printf '1.0.0\n'; }
    _validation_token() { printf 'v2:head:full:reduced\n'; }
    _run_isolated() { printf 'fallback=%s\n' "$6"; }
    GOVNA_PREP_VALIDATION_TOKEN='v2:head:full:reduced' \
      prep_run 0 0 0 v1.0.1 release
  )
  assert_contains "$output" 'validation evidence: current'
  assert_contains "$output" 'fallback=0'

  output=$(
    _validate_release_inputs() { return 0; }
    _validate_git_state() { return 0; }
    _validate_canon_version_bump() { return 0; }
    _cargo_version_info() { printf '1.0.0\n'; }
    _ac_refs() { printf ''; }
    _matching_ac_files() { printf ''; }
    _govna_program_version_info() { printf '1.0.0\n'; }
    _validation_token() { printf 'v2:head:full:reduced\n'; }
    _run_isolated() { printf 'fallback=%s\n' "$6"; }
    GOVNA_PREP_VALIDATION_TOKEN='malformed' prep_run 0 0 0 v1.0.1 release 2>&1
  )
  assert_contains "$output" 'missing or stale'
  assert_contains "$output" 'fallback=1'

  output=$(
    _validate_release_inputs() { return 0; }
    _validate_git_state() { return 0; }
    _validate_canon_version_bump() { return 0; }
    _cargo_version_info() { printf '1.0.0\n'; }
    _ac_refs() { printf ''; }
    _matching_ac_files() { printf ''; }
    _govna_program_version_info() { printf '1.0.0\n'; }
    _validation_token() { printf 'v2:head:full:reduced\n'; }
    _run_isolated() { printf 'fallback=%s\n' "$6"; }
    GOVNA_PREP_VALIDATION_TOKEN='' prep_run 0 0 0 v1.0.1 release 2>&1
  )
  assert_contains "$output" 'missing or stale'
  assert_contains "$output" 'fallback=1'
  pass
}

test_fallback_failure_precedes_mutation() {
  local fixture rc
  fixture=$(new_fixture) || fail 'create fallback fixture'
  set +e
  (
    _prep_phase() { return 1; }
    _replace_cargo_version() { printf changed >"$fixture/mutated"; }
    _prep_apply 1.0.1 release '' '' 1
  )
  rc=$?
  set -e
  assert_equal "$rc" 1
  [ ! -e "$fixture/mutated" ] || fail 'fallback failure allowed mutation'
  rm -rf -- "$fixture"
  pass
}

test_prep_phase_output_modes() {
  local fixture output rc
  fixture=$(new_fixture) || fail 'create phase-output fixture'
  _cargo_target="$fixture"
  _prep_phase_index=0
  phase_ok() { printf 'complete phase detail\n'; }
  phase_fail() { printf 'failure detail\n'; return 7; }

  _prep_verbose=0
  output=$(_prep_phase 'quiet phase' phase_ok 2>&1)
  assert_contains "$output" 'quiet phase: passed'
  assert_not_contains "$output" 'complete phase detail'
  set +e
  output=$(_prep_phase 'failed phase' phase_fail 2>&1)
  rc=$?
  set -e
  assert_equal "$rc" 7
  assert_contains "$output" 'failure detail'

  _prep_verbose=1
  output=$(_prep_phase 'verbose phase' phase_ok 2>&1)
  assert_contains "$output" 'complete phase detail'
  rm -rf -- "$fixture"
  pass
}

test_successful_full_build_emits_token() {
  local output
  output=$(
    _load_bin_targets() { return 0; }
    _require_cargo() { return 0; }
    _run_isolated() { return 0; }
    _validation_token() { printf 'v2:head:full:reduced\n'; }
    _next_patch_tag() { return 0; }
    build_run 0
  )
  assert_contains "$output" '==> Validation token:'
  assert_contains "$output" 'v2:head:full:reduced'
  pass
}

test_failed_full_build_omits_token() {
  local output rc
  set +e
  output=$(
    _load_bin_targets() { return 0; }
    _require_cargo() { return 0; }
    _run_isolated() { return 9; }
    _validation_token() { printf 'v2:head:full:reduced\n'; }
    build_run 0 2>&1
  )
  rc=$?
  set -e
  assert_equal "$rc" 9
  assert_not_contains "$output" 'Validation token'
  pass
}

test_prep_verbose_aliases() {
  local short long
  short=$(
    prep_run() { printf '%s:%s:%s\n' "$1" "$2" "$3"; }
    prep_main -v v1.0.1 release
  )
  long=$(
    prep_run() { printf '%s:%s:%s\n' "$1" "$2" "$3"; }
    prep_main --verbose v1.0.1 release
  )
  assert_equal "$short" "$long"
  assert_equal "$short" '0:0:1'
  pass
}

test_owned_target_cleanup_paths() {
  local fixture target output rc
  fixture=$(new_fixture) || fail 'create cleanup fixture'
  target=$(mktemp -d "${TMPDIR:-/tmp}/govna-rust-target.XXXXXX") ||
    fail 'create owned cleanup target'
  _repo_root="$fixture"
  _cargo_target="$target"
  _cargo_target_owned=1
  _cleanup_cargo_target || fail 'owned target success cleanup failed'
  [ ! -e "$target" ] || fail 'owned target remained after cleanup'

  target=$(mktemp -d "${TMPDIR:-/tmp}/govna-rust-target.XXXXXX") ||
    fail 'create signal cleanup target'
  set +e
  (
    _repo_root="$fixture"
    _cargo_target="$target"
    _cargo_target_owned=1
    _cargo_signal 130
  )
  rc=$?
  set -e
  assert_equal "$rc" 130
  [ ! -e "$target" ] || fail 'owned target remained after signal cleanup'

  target=$(mktemp -d "${TMPDIR:-/tmp}/govna-rust-target.XXXXXX") ||
    fail 'create failed cleanup target'
  set +e
  output=$(
    _create_cargo_target() {
      _repo_root="$fixture"
      _cargo_target="$target"
      _cargo_target_owned=1
    }
    rm() { return 1; }
    _run_isolated true 2>&1
  )
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" "$target"
  assert_contains "$output" 'remove it manually'
  rm -rf -- "$target" "$fixture"
  pass
}

test_utility_declaration_validation
test_compiled_version_output
test_manifest_path_mapping
test_install_reporting
test_prep_no_build_rejection
test_validation_token_tracks_git_visible_state
test_baseline_validation_token_refresh
test_refresh_validation_token_cli_contract
test_shared_cargo_target_ownership
test_prep_evidence_routing
test_fallback_failure_precedes_mutation
test_prep_phase_output_modes
test_successful_full_build_emits_token
test_failed_full_build_omits_token
test_prep_verbose_aliases
test_owned_target_cleanup_paths

printf 'build CLI tests: %d passed\n' "$test_count"
