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
    _cargo_version_info() { printf '1.0.0\n'; }
    _ac_refs() { printf ''; }
    _matching_ac_files() { printf ''; }
    prep_run 0 1 v1.0.1 release 2>&1
  )
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" 'cannot use --no-build/-B'
  pass
}

test_utility_declaration_validation
test_compiled_version_output
test_manifest_path_mapping
test_install_reporting
test_prep_no_build_rejection

printf 'build CLI tests: %d passed\n' "$test_count"
