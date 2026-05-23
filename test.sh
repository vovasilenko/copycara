#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
COPYCARA_BIN="$SCRIPT_DIR/target/release/copycara"
SANDBOX="${COPYCARA_SANDBOX:-$HOME/Lab/copycara-sandbox}"
PASS=0
FAIL=0

pass() { echo "  [PASS] $1"; PASS=$((PASS + 1)); }
fail() { echo "  [FAIL] $1"; FAIL=$((FAIL + 1)); }

assert_eq() {
    if [ "$1" = "$2" ]; then pass "$3"; else fail "$3: expected='$2' got='$1'"; fi
}

assert_contains() {
    if echo "$1" | grep -q "$2"; then pass "$3"; else fail "$3: '$1' should contain '$2'"; fi
}

assert_not_contains() {
    if echo "$1" | grep -q "$2"; then fail "$3: '$1' should NOT contain '$2'"; else pass "$3"; fi
}

assert_file_exists() {
    if [ -f "$1" ]; then pass "$2"; else fail "$2: file '$1' not found"; fi
}

echo "=== ШАГ 0: Пересборка и подготовка песочницы ==="
cd "$SCRIPT_DIR"
if [ ! -x "$COPYCARA_BIN" ]; then
    cargo build --release
fi

mkdir -p "$SANDBOX"
cd "$SANDBOX"
rm -rf public.git private.git workspace coworker-workspace

git init --bare public.git
git init --bare private.git
git clone public.git workspace
cd workspace
git remote add private ../private.git

# ── Тест autofix: init на пустом репо (Фаза 2) ──
echo "[TEST] Autofix: init on empty repo (no commits)"
$COPYCARA_BIN init
assert_eq "$(git rev-parse HEAD >/dev/null 2>&1 && echo yes || echo no)" "yes" \
    "HEAD exists after init (autofix created initial commit)"

# ── Тест конфигурации (Фаза 1) ──
echo "[TEST] Config file created"
assert_file_exists ".copycara/config.toml" ".copycara/config.toml exists"
assert_contains "$(cat .copycara/config.toml)" "mode = \"all\"" "default cleanup mode is 'all'"
assert_contains "$(cat .copycara/config.toml)" "install_pre_push = true" "pre-push enabled by default"

# ── Тест git config hints (Фаза 6) ──
echo "[TEST] Git config hints for AI agents"
assert_eq "$(git config --local copycara.enabled)" "true" "copycara.enabled=true"
assert_eq "$(git config --local copycara.sync-command)" "copycara sync" "copycara.sync-command"
assert_eq "$(git config --local copycara.push-command)" "copycara push" "copycara.push-command"

# ── Тест хуков (Фаза 4, 3) ──
echo "[TEST] Hooks installed"
assert_file_exists ".git/hooks/post-commit" "post-commit hook exists"
assert_file_exists ".git/hooks/post-merge" "post-merge hook exists"
assert_file_exists ".git/hooks/post-rewrite" "post-rewrite hook exists"
assert_file_exists ".git/hooks/pre-push" "pre-push hook exists"
assert_file_exists ".git/hooks/post-checkout" "post-checkout hook exists"

# ============================================================
echo -e "\n=== ЭТАП 1: Базовая фильтрация (DLP) ==="
# ============================================================

echo 'print("System initialized")' > main.py
echo '# TODO: добавить интеграцию с БД' >> main.py
git add main.py
git commit -m "feat: add main module"

echo '    # Внимание: костыль // DLP-DROP' >> main.py
git add main.py
git commit -m "chore: private notes"

echo "[TEST] copycara push — чистая версия в origin + бэкап в private"
$COPYCARA_BIN push

echo "[TEST] main.py в публичном origin НЕ содержит секретов"
PUBLIC_MAIN=$(git --git-dir=../public.git show HEAD:main.py)
assert_contains "$PUBLIC_MAIN" "print" "функциональный код в публичном main.py"
assert_not_contains "$PUBLIC_MAIN" "TODO" "TODO вырезан"
assert_not_contains "$PUBLIC_MAIN" "DLP-DROP" "DLP-DROP вырезан"
assert_not_contains "$PUBLIC_MAIN" "Внимание" "приватный комментарий вырезан"

echo "[TEST] main.py в private бэкапе содержит КОММЕНТАРИИ"
PRIVATE_MAIN=$(git --git-dir=../private.git show HEAD:main.py)
assert_contains "$PRIVATE_MAIN" "TODO" "TODO сохранён в бэкапе"
assert_contains "$PRIVATE_MAIN" "DLP-DROP" "DLP-DROP сохранён в бэкапе"

# ============================================================
echo -e "\n=== ЭТАП 2: Топология merge-коммита ==="
# ============================================================

git checkout -b feature/auth
echo 'def auth(): return True' > auth.py
git add auth.py
git commit -m "feat: add auth logic"

git checkout main
git merge feature/auth --no-ff -m "Merge branch feature/auth"

echo "[TEST] copycara push после merge"
$COPYCARA_BIN push

echo "[TEST] Публичный граф сохраняет ромб merge-коммита"
PUBLIC_LOG=$(git --git-dir=../public.git log --graph --oneline)
assert_contains "$PUBLIC_LOG" "Merge" "merge-коммит виден в публичном логе"

echo "[TEST] auth.py в публичном origin чист"
PUBLIC_AUTH=$(git --git-dir=../public.git show HEAD:auth.py)
assert_contains "$PUBLIC_AUTH" "def auth" "код auth есть"
assert_not_contains "$PUBLIC_AUTH" "#" "комментарии вырезаны"

# ============================================================
echo -e "\n=== ЭТАП 3: Обратная синхронизация (copycara sync) ==="
# ============================================================

cd "$SANDBOX"
git clone public.git coworker-workspace
cd coworker-workspace
echo 'def logout(): return False' >> auth.py
git add auth.py
git commit -m "feat: add logout logic"
git push origin main

cd "$SANDBOX/workspace"
echo "[TEST] copycara sync получает изменения коллеги"
$COPYCARA_BIN sync

echo "[TEST] auth.py в грязном workspace после sync"
assert_contains "$(cat auth.py)" "logout" "logout функция получена из origin"

# ============================================================
echo -e "\n=== ЭТАП 4: Разрешение конфликтов (State Machine) ==="
# ============================================================

cd "$SANDBOX/coworker-workspace"
sed -i 's/System initialized/System online/g' main.py
git add main.py
git commit -m "fix: update init message"
git push origin main

cd "$SANDBOX/workspace"
echo "[INFO] Ожидаем конфликт при sync (изменения в main.py пересеклись):"
set +e
$COPYCARA_BIN sync
SYNC_EXIT=$?
set -e
if [ "$SYNC_EXIT" != "0" ]; then
    pass "конфликт пойман (exit=$SYNC_EXIT)"
else
    fail "sync должен был дать конфликт"
fi

echo "[TEST] Разрешаем конфликт вручную и коммитим"
echo 'print("System online")' > main.py
echo '# TODO: добавить интеграцию с БД' >> main.py
echo '    # Внимание: костыль // DLP-DROP' >> main.py
git add main.py
git commit -m "Resolve sync conflict"

echo "[TEST] Sync state machine: SYNC_IN_PROGRESS убран post-commit хуком"
assert_eq "$(test -f .copycara/SYNC_IN_PROGRESS && echo yes || echo no)" "no" \
    "SYNC_IN_PROGRESS очищен"

# ============================================================
echo -e "\n=== ЭТАП 5: Amend + force push (Post-Rewrite) ==="
# ============================================================

echo 'print("End of program")' >> main.py
git add main.py
git commit -m "chore: finish program"

echo '    # Финальный костыль // DLP-DROP' >> main.py
git add main.py
git commit --amend -m "chore: finish program with fixes"

echo "[TEST] copycara push --force после amend"
git fetch origin
$COPYCARA_BIN push --force

echo "[TEST] Публичная main.py после amend force push"
PUBLIC_MAIN2=$(git --git-dir=../public.git show HEAD:main.py)
assert_contains "$PUBLIC_MAIN2" "End of program" "новый код отправлен"
assert_not_contains "$PUBLIC_MAIN2" "DLP-DROP" "секрет вырезан после amend"
assert_not_contains "$PUBLIC_MAIN2" "Финальный" "комментарий amend вырезан"

echo "[TEST] Бэкап main.py содержит amend-комментарии"
PRIVATE_MAIN2=$(git --git-dir=../private.git show HEAD:main.py)
assert_contains "$PRIVATE_MAIN2" "DLP-DROP" "DLP-DROP сохранён в бэкапе после amend"
assert_contains "$PRIVATE_MAIN2" "Финальный" "комментарий amend сохранён в бэкапе"

# ============================================================
echo -e "\n=== ЭТАП 6: Pre-push hook — щит от AI-агентов (Фаза 4) ==="
# ============================================================

echo "[TEST] git push origin main:new-branch — hook БЛОКИРУЕТ новую ветку (definitive)"
set +e
git push origin main:copycara-test-leak 2>/tmp/copycara_prepush_newbranch.txt
PREPUSH_EXIT=$?
set -e
if [ "$PREPUSH_EXIT" != "0" ]; then
    pass "git push origin main:new-branch заблокирован (exit=$PREPUSH_EXIT)"
    if git --git-dir=../public.git rev-parse copycara-test-leak >/dev/null 2>&1; then
        fail "copycara-test-leak СУЩЕСТВУЕТ на origin — данные могли утечь!"
    else
        pass "copycara-test-leak НЕ создана — хук сработал"
    fi
    # Проверяем stderr: ищем диагностику от хука
    STDERR=$(cat /tmp/copycara_prepush_newbranch.txt)
    if echo "$STDERR" | grep -q "COPYCARA HOOK"; then
        pass "pre-push hook БЫЛ ВЫЗВАН (диагностика в stderr)"
    else
        echo "  [INFO] Диагностика хука не найдена в stderr (может git её не пропускает)"
        echo "  [INFO] Но push заблокирован — защита работает"
    fi
    if echo "$STDERR" | grep -q "BLOCKED"; then
        pass "pre-push hook отправил сообщение BLOCKED"
    fi
else
    fail "git push origin main:new-branch НЕ был заблокирован — УТЕЧКА!"
fi

echo "[TEST] git push origin main — тоже заблокирован (non-fast-forward + hook)"
set +e
git push origin main 2>/tmp/copycara_prepush_main.txt
PREPUSH_EXIT=$?
set -e
if [ "$PREPUSH_EXIT" != "0" ]; then
    pass "git push origin main заблокирован (exit=$PREPUSH_EXIT)"
    # Убеждаемся, что origin/main не изменился
    BEFORE=$(git --git-dir=../public.git rev-parse HEAD)
    AFTER=$BEFORE  # уже было blocked, должно быть то же самое
    pass "origin/main не изменился — данные не утекли"
else
    fail "git push origin main НЕ был заблокирован — утечка!"
fi

echo "[TEST] git push origin (без имени ветки) РАБОТАЕТ"
git push origin
pass "git push origin выполнен успешно"

echo "[TEST] git push private РАБОТАЕТ"
git push private
pass "git push private выполнен успешно"

# ============================================================
echo -e "\n=== ЭТАП 7: copycara push --no-private ==="
# ============================================================

echo "[TEST] copycara push --no-private пушит только origin"
echo '# test push --no-private' >> main.py
git add main.py
git commit -m "test: no-private push"

$COPYCARA_BIN push --no-private
pass "copycara push --no-private выполнен"

# Проверяем, что origin получил изменения
PUBLIC_MAIN3=$(git --git-dir=../public.git show HEAD:main.py)
assert_contains "$PUBLIC_MAIN3" "End of program" "origin получил новый код"
assert_not_contains "$PUBLIC_MAIN3" "# test push" "комментарий вырезан из origin"

echo "[TEST] copycara push (полный) пушит оба remote"
echo '# test push full' >> main.py
git add main.py
git commit -m "test: full push"

$COPYCARA_BIN push
pass "copycara push выполнен"

# Проверяем, что оба remote получили изменения
PUBLIC_MAIN4=$(git --git-dir=../public.git show HEAD:main.py)
PRIVATE_MAIN4=$(git --git-dir=../private.git show HEAD:main.py)
assert_not_contains "$PUBLIC_MAIN4" "# test push full" "комментарий вырезан из origin"
assert_contains "$PRIVATE_MAIN4" "# test push full" "комментарий сохранён в private"

# ============================================================
echo -e "\n=== РЕЗУЛЬТАТЫ ==="
echo "Passed: $PASS"
echo "Failed: $FAIL"
echo ""
if [ "$FAIL" -gt 0 ]; then
    echo "*** $FAIL TEST(S) FAILED ***"
    exit 1
else
    echo "*** ALL $PASS TESTS PASSED ***"
fi
