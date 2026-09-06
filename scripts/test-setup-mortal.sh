#!/usr/bin/env bash
# 使用临时 Git 仓库验证安装前的版本与工作区检查，不下载或编译 Mortal。
set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
test_dir="$(mktemp -d "${TMPDIR:-/tmp}/kyoku-setup-test.XXXXXX")"
trap 'rm -rf "$test_dir"' EXIT
runtime_dir="$test_dir/mortal/runtime"
mkdir -p "$runtime_dir" "$test_dir/scripts"
git -C "$runtime_dir" init --quiet
git -C "$runtime_dir" config user.name 'Setup test'
git -C "$runtime_dir" config user.email 'setup-test@example.invalid'
git -C "$runtime_dir" config commit.gpgsign false
# 即使用户隐藏未跟踪文件，安装检查也必须能发现它们。
git -C "$runtime_dir" config status.showUntrackedFiles no
printf 'original\n' > "$runtime_dir/source.py"
printf '/target/\n*.so\n__pycache__/\n' > "$runtime_dir/.gitignore"
git -C "$runtime_dir" add source.py .gitignore
git -C "$runtime_dir" commit --quiet -m 'Test fixture'
fixture_commit="$(git -C "$runtime_dir" rev-parse HEAD)"
sed "s/^runtime_commit=.*/runtime_commit=$fixture_commit/" \
    "$script_dir/setup-mortal.sh" > "$test_dir/scripts/setup-mortal.sh"

# 到达 Python 步骤即停止；86 表示已通过源码检查，避免创建环境或联网。
printf '#!/usr/bin/env bash\nexit 86\n' > "$test_dir/python"
chmod +x "$test_dir/python"

check_setup() {
    local expected="$1" status=0
    bash "$test_dir/scripts/setup-mortal.sh" "$test_dir/python" > "$test_dir/output" 2>&1 || status=$?
    if [[ "$status" -ne "$expected" ]]; then
        cat "$test_dir/output" >&2
        echo "Expected exit $expected, got $status" >&2
        exit 1
    fi
    if [[ "$expected" -eq 1 ]]; then
        if ! grep -q "$2" "$test_dir/output"; then
            cat "$test_dir/output" >&2
            echo "Expected diagnostic: $2" >&2
            exit 1
        fi
    fi
}

check_setup 86
printf 'changed\n' >> "$runtime_dir/source.py"
check_setup 1 'uncommitted or untracked changes'
git -C "$runtime_dir" add source.py
check_setup 1 'uncommitted or untracked changes'
git -C "$runtime_dir" restore --source=HEAD --staged --worktree source.py
printf 'extra\n' > "$runtime_dir/extra.py"
check_setup 1 'uncommitted or untracked changes'
rm "$runtime_dir/extra.py"

mkdir -p "$runtime_dir/target" "$runtime_dir/__pycache__"
touch "$runtime_dir/target/build" "$runtime_dir/libriichi.so" "$runtime_dir/__pycache__/source.pyc"
check_setup 86
git -C "$runtime_dir" commit --quiet --allow-empty -m 'Wrong revision'
check_setup 1 'Unexpected Mortal revision'
echo 'Mortal setup checks passed (clean, unstaged, staged, untracked, ignored artifacts, wrong revision).'
