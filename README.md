# Copycara: Topological Git DLP Engine

[![CI](https://github.com/vovasilenko/copycara/actions/workflows/ci.yml/badge.svg)](https://github.com/vovasilenko/copycara/actions/workflows/ci.yml)

**Copycara** — локальный Git-движок Data Loss Prevention (DLP). Он автоматически вырезает приватные комментарии (PACM, GRACE, семантические якоря, Belief States, TODO, FIXME) перед отправкой кода в публичный репозиторий, сохраняя полную топологию Git-графа и создавая приватный бэкап оригиналов.

Copycara не требует от разработчика менять привычки: `git add` / `git commit` / `git push` работают как обычно, а очистка происходит автоматически через git hooks. Для AI-агентов установлены хуки-щиты, блокирующие опасные операции (прямой пуш грязной ветки) с понятным сообщением об ошибке.

---

## ⚡ Быстрый старт

```bash
# 1. Установка
git clone <ваш-репозиторий> workspace
cd workspace

# 2. Настройка remote (если ещё не настроены)
git remote add origin <публичный-URL>
git remote add private <приватный-URL>

# 3. Инициализация Copycara
copycara init

# 4. Работа как обычно
echo 'print("hello")' > main.py
git add main.py
git commit -m "feat: init"
copycara push           # публикует чистый код + бэкап
```

> Если в репозитории ещё нет коммитов — `copycara init` сам создаст пустой коммит.

---

## 🏗 Архитектура

### Две плоскости

| Плоскость | Где находится | Что содержит |
|-----------|--------------|--------------|
| **Dirty Plane** (Workspace) | Ваша рабочая директория | Оригинальный код с комментариями, TODO, методологическими тегами |
| **Clean Plane** (Mirror) | `.copycara/mirror` | Стерильная копия без комментариев |

### Очистка (DLP)

Движок использует библиотеку **uncomment**, построенную на **tree-sitter AST**:

- Парсит исходный код синтаксически, а не регулярками
- Не повреждает строковые литералы (`print("// не комментарий")`)
- Поддерживает все языки tree-sitter: Python, Rust, JS, TS, C++, Go, Java, C#, Ruby, Bash и десятки других
- Управляется файлом `.copycara/config.toml`

Если коммит содержит только комментарии — он полностью отбрасывается из теневой истории (Drop Empty Commit).

### Маршрутизация (Refspecs)

При `copycara init` настраиваются refspecs, которые автоматически подменяют ветки при пуше:

```
remote.origin.push = refs/copycara/heads/*:refs/heads/*
```

Когда вы делаете `git push origin` (без имени ветки), Git вместо вашей грязной `refs/heads/main` отправляет чистую `refs/copycara/heads/main`. Публичный сервер никогда не видит ваши комментарии.

### Карта соответствия (Git Notes)

Связь между грязным и чистым коммитом хранится в `git notes` по ссылке `refs/notes/copycara-map`. При push в `private` эта карта бэкапируется вместе с оригинальным кодом.

---

## 📦 Команды Copycara

### `copycara init`

Инициализирует репозиторий: настраивает refspecs, создаёт `.copycara/mirror`, устанавливает хуки, конфиг и git config hints для AI-агентов.

```bash
copycara init
```

Что делает под капотом:

| Шаг | Действие |
|-----|----------|
| 0 | Если HEAD отсутствует — создаёт `git commit --allow-empty` (autofix) |
| 1 | Настраивает `remote.origin.push` (shadow → clean) и `remote.private.push` (dirty + notes) |
| 2 | Создаёт `.copycara/mirror` (worktree), записывает `.copycara/config.toml` |
| 3 | Устанавливает 5 git hooks (post-commit, post-merge, post-rewrite, pre-push, post-checkout) |
| 4 | Перенаправляет upstream текущей ветки на `private` (или отключает tracking на `origin`) |
| 5 | Записывает `git config --local copycara.{enabled,sync-command,push-command}` |

### `copycara push`

Безопасно публикует код в оба remote.

```bash
copycara push                          # чистый код → origin + бэкап → private
copycara push --force                  # с --force-with-lease (после amend)
copycara push --no-private             # только origin, без бэкапа
```

Под капотом:

1. `git push origin` — пушит `refs/copycara/heads/* → refs/heads/*` (чистый код)
2. `git push private` — пушит dirty refs + `refs/notes/copycara-map` (бэкап)

> `--force` использует `--force-with-lease`, а не `--force`: если кто-то другой изменил ветку на сервере с момента вашего последнего fetch — пуш будет отклонён.

### `copycara sync`

Получает изменения коллег из `origin` и накладывает их на ваш грязный workspace.

```bash
copycara sync
```

Как работает:

1. Fetch из `origin/current_branch`
2. Merge в `.copycara/mirror` (чистый граф)
3. Diff между старым и новым чистым состоянием
4. 3-way merge патча в ваш workspace

Если возникает конфликт — sync останавливается, записывается `.copycara/SYNC_IN_PROGRESS`. Вы разрешаете конфликт и коммитите — хук автоматически завершает синхронизацию.

### `copycara uninstall`

Полностью удаляет Copycara из репозитория.

```bash
copycara uninstall
```

Удаляет: refspecs, `.copycara/`, все 5 хуков, git config hints.

---

## 🛠 Повседневный Workflow

### Коммиты

```bash
git add .
git commit -m "feat: implement auth with GRACE anchors"
```
> Комментарии в коде автоматически вырезаются post-commit хуком.

### Отправка изменений

| Команда | Что делает |
|---------|-----------|
| `copycara push` | **(рекомендуется)** Чистый код → origin + бэкап → private |
| `git push origin` | Только чистый код → origin (refspec подменяет ветку) |
| `git push private` | Только грязный бэкап → private |
| `copycara push --force` | После amend: force push с `--force-with-lease` |

> `git push origin main` — **запрещено**. Pre-push hook заблокирует с сообщением.

### Изменение истории

```bash
git commit --amend -m "new message"
```

Хук `post-rewrite` создаёт новый теневой коммит, обновляет `refs/copycara/heads/main` и карту `git notes`.

После amend push с `--force` обязателен, так как теневая история разошлась:

```bash
copycara push --force
```

### Получение изменений

```bash
copycara sync
```

Заменяет `git pull`. Не используйте `git pull` — он смешает чистый граф с грязным.

### Ветвление

```bash
git checkout -b feature/new
# post-checkout hook сам настроит upstream на private (или снимет tracking с origin)
git push private       # бэкап новой ветки
```

---

## 🚨 Защита AI-агентов

### Активные щиты

| Попытка | Реакция | Результат |
|---------|---------|-----------|
| `git push origin main` | Pre-push hook: **BLOCKED** + инструкция | Пуш отклонён, данные в безопасности |
| `git push origin feature/x` | Pre-push hook: **BLOCKED** + инструкция | Пуш отклонён |
| `git pull` | upstream настроен на `private` | Тянет грязный бэкап (безопасно) |
| `git pull origin main` | Вливает чистый код в dirty workspace | Не рекомендуется, но данные не утекают |
| Новый проект (`git init`, `copycara init`) | Autofix: пустой коммит | Работает без ошибок |

### AI Agent Directives

| Запрещено | Правильная команда | Причина |
|-----------|-------------------|---------|
| `git push origin <branch>` | `git push origin` или `copycara push` | Прямой пуш отправляет грязный код (с комментариями) в публичный репозиторий |
| `git pull` | `copycara sync` | `git pull` смешивает чистый и грязный графы |
| `git pull origin <branch>` | `copycara sync` | Аналогично |

### Git config hints

Copycara записывает в локальный git config ключи. Вы можете их прочитать:

```bash
git config --local --list | grep copycara
# copycara.enabled=true
# copycara.sync-command=copycara sync
# copycara.push-command=copycara push
```

---

## ⚙️ Конфигурация (`.copycara/config.toml`)

Создаётся автоматически при `copycara init`. Ниже — полная структура и практические примеры.

### Все поля

```toml
[cleanup]
# "all" — удалять ВСЕ комментарии | "smart" — сохранять TODO/FIXME/doc
mode = "all"

# Расширения, которых нет в списке valid_exts, но их тоже надо чистить.
# extension_map (см. ниже) обычно удобнее.
extra_extensions = []

# Комментарии, содержащие эти подстроки, НЕ вырезаются (сохраняются как есть)
preserve_patterns = ["COPYCARA-KEEP", "NO-DLP"]

# Маппинг расширений, которые tree-sitter не знает, на известные.
# Работает через rename-trick: файл переименовывается, чистится, и
# переименовывается обратно.
# Пример: .cu (CUDA C++) → .cpp
extension_map = { cu = "cpp", cuh = "cpp" }

[push]
# Автоматический пуш в private remote при copycara push
auto_push_private = true

# Использовать --force-with-lease (безопасный force) при copycara push --force
force_with_lease = true

[hooks]
# Установить pre-push хук (блокировка git push origin <branch>)
install_pre_push = true

# Установить post-checkout хук (автонастройка upstream для новых веток)
install_post_checkout = true
```

### Пример 1: CUDA C++ (`.cu`, `.cuh`)

tree-sitter не знает про `.cu` — это CUDA C++. Если попробовать,
uncomment скажет `Unsupported file type: kernel.cu`.

Раньше вы обходили это ручным переименованием `.cu → .cpp`. Теперь это
делает конфиг:

```toml
[cleanup]
extension_map = { cu = "cpp", cuh = "cpp" }
```

Copycara перед очисткой переименует файл из `kernel.cu` в `kernel.cpp`,
uncomment распарсит его как C++ и вырежет комментарии, затем переименует
обратно в `kernel.cu`.

### Пример 2: Smart mode — сохраняем TODO и доку-комментарии

```toml
[cleanup]
mode = "smart"
```

В `"smart"` режиме Copycara **не вырезает**:
- `TODO`, `todo`
- `FIXME`, `fixme`
- doc-комментарии (`///`, `/** */`, `#| ... |#` и т.д.)

Удаляются только обычные комментарии:
```python
# Этот комментарий будет удалён
x = 1
# TODO: эта метка СОХРАНИТСЯ в публичном коде
y = 2
```

### Пример 3: Паттерны сохранения

```toml
[cleanup]
preserve_patterns = ["COPYCARA-KEEP", "NO-DLP", "PUBLIC-OK"]
```

Комментарии, содержащие любое из этих слов, не удаляются:
```python
# COPYCARA-KEEP: эта метка не вырежется
# PUBLIC-OK: этот комментарий останется в публичном коде
# А этот — удалится
```

### Пример 4: Несколько неизвестных расширений

```toml
[cleanup]
extension_map = { cu = "cpp", cuh = "cpp", metal = "cpp", cl = "c" }
```

- `.metal` (Apple Metal Shading Language) → C++
- `.cl` (OpenCL) → C
- `.cu` / `.cuh` (CUDA C++) → C++

### Пример 5: Kotlin / Swift (если не хватает valid_exts)

```toml
[cleanup]
extra_extensions = ["kt", "kts", "swift"]
```

`extra_extensions` просто добавляет расширения в белый список —
tree-sitter сам должен знать эти языки. Если не знает — используйте
`extension_map`.

### Пример 6: Полный агрессивный режим

```toml
[cleanup]
mode = "all"
# Ничего не сохраняем — даже если кто-то поставил ~keep
preserve_patterns = []

[hooks]
install_pre_push = true
```

---

## 🪝 Система хуков

| Хук | Событие | Действие |
|-----|---------|----------|
| **post-commit** | После каждого коммита | Очищает код в `.copycara/mirror` через uncomment, создаёт теневой коммит, обновляет `git notes` |
| **post-merge** | После merge (в т.ч. `git pull`) | Аналогично post-commit |
| **post-rewrite** | После amend / rebase | Создаёт новый теневой коммит, обновляет refspec |
| **pre-push** | Перед `git push origin` | Блокирует `git push origin <branch>` (грязные ветки), пропускает `git push origin` (shadow-рефы) и `git push private` |
| **post-checkout** | После checkout новой ветки | Настраивает upstream на private (или отключает tracking на origin) |

---

## ⚠️ Тонкие моменты

### `non-fast-forward` при пуше

**Симптом:** `Updates were rejected because the tip of your current branch is behind...`

**Причина:** Теневая история разошлась (например, после `amend` или `rebase` — post-rewrite пересоздаёт теневой коммит, и старый на сервере становится неактуальным).

**Решение:** `copycara push --force`. Использует `--force-with-lease` — безопасен при командной работе.

### Сообщение `diverged` в `git status`

**Причина:** Локальный `main` трекает `origin/main`, но на сервере лежат чистые коммиты (другие хэши).

**Решение:** `copycara init` автоматически перенаправляет upstream на `private/main` или отключает tracking. Если проблема осталась:

```bash
git branch --set-upstream-to=private/main main
```

### Пустой репозиторий

`copycara init` сам создаёт пустой коммит, если HEAD отсутствует. Ручной `git commit --allow-empty` не требуется.

### Новые типы файлов / языки

tree-sitter поддерживает большинство языков автоматически. Если файл не обрабатывается — проверьте расширение в списке `valid_exts` в `apply_dlp_cleanup()` или используйте `extra_extensions` в `.copycara/config.toml`.

### После разрешения конфликтов sync

Входящий код приходит из чистого графа — в нём нет комментариев и тегов. После разрешения конфликтов проверьте код и при необходимости добавьте методологические теги обратно.

---

## 📋 Quick Reference

```bash
# Инициализация
copycara init                          # настроить Copycara в репозитории
copycara uninstall                     # удалить Copycara

# Ежедневная работа
git add . && git commit -m "msg"       # как обычно
copycara push                          # отправить чистый код + бэкап
copycara push --force                  # после amend (с --force-with-lease)
copycara sync                          # получить изменения коллег (вместо git pull)

# Работа с private бэкапом
git push private                       # только бэкап

# Если случайно написали
git push origin main                   # ❌ заблокировано pre-push hook
git push origin                        # ✅ чистая публикация через refspec

# См. также
git config --local --list | grep copycara  # git config hints для AI-агентов
```

---

## 🧪 Тестирование

```bash
./test.sh
# Passed: 42, Failed: 0
```

Создаёт песочницу в `~/Lab/copycara-sandbox/` с двумя bare-репозиториями (public + private), клонирует workspace, инициализирует Copycara и прогоняет 7 этапов тестирования: DLP-фильтрация, топология merge, reverse sync, конфликты, amend + force push, pre-push hook, push variants.
