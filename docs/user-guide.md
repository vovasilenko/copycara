# Copycara + Git: руководство пользователя

Это руководство объясняет, как работать с Git и Copycara вместе. Оно написано для тех,
кто не является экспертом в Git, но хочет понимать, что происходит.

---

## 1. Git-основы, которые нужно знать для Copycara

### 1.1. Remote (удалённый репозиторий)

Remote — это «адрес» другого репозитория. Команда `git remote -v` показывает все адреса:

```
origin  → git@flight-control.ru:ai-codec.git    (публичный, чистый код)
local   → git@gitlab.amulet-s.ru:nvc-rt.git      (публичный, чистый код)
private → git@github.com:vovasilenko/ai-codec.git (приватный бэкап)
```

### 1.2. Fetch, Pull, Push — в чём разница

| Команда | Что делает | Когда использовать |
|---------|-----------|-------------------|
| `git fetch <remote>` | Скачивает ВСЕ изменения с remote, но НЕ меняет твои файлы | Чтобы увидеть, что изменилось на сервере |
| `git pull <remote>` | fetch + merge в текущую ветку | Чтобы получить изменения И применить их |
| `git push <remote>` | Отправляет твои изменения на remote | Чтобы поделиться кодом |

**Главное правило Copycara:** не используй `git pull` — используй `copycara sync`.
Не используй `git push origin <ветка>` — используй `copycara push`.

### 1.3. Ветки (branches)

Ветка — это независимая линия разработки. Команды:

```bash
git branch                          # показать все локальные ветки
git branch -a                       # показать ВСЕ ветки (включая удалённые)
git checkout <ветка>                # переключиться на ветку
git checkout -b <новая-ветка>       # создать И переключиться
git merge <ветка>                   # влить другую ветку в текущую
```

### 1.4. Как checkout находит ветку

Если ты делаешь `git checkout feat/integerization`, Git ищет ветку `feat/integerization`
среди всех remote. Если ветка есть на нескольких remote (например, на `origin` и на
`private`), Git выдаст ошибку:

```
fatal: 'feat/integerization' matched multiple (2) remote tracking branches
```

**Решение:** укажи явно, с какого remote брать:

```bash
git checkout -b feat/integerization private/feat/integerization
```

### 1.5. Fast-forward vs merge commit

Когда ты делаешь `git merge <другая-ветка>`, Git может сделать две вещи:

**Fast-forward (быстрая перемотка):** если текущая ветка — прямой предок другой ветки,
Git просто передвинет указатель. Новый коммит НЕ создаётся.

```
До:     A---B  (текущая)
             \
              C---D  (другая ветка)

После FF: A---B---C---D  (текущая = D)
```

**Merge commit (коммит слияния):** если ветки разошлись, Git создаёт новый коммит,
объединяющий обе линии.

```
До:     A---B---E  (текущая)
         \
          C---D  (другая ветка)

После:  A---B---E---F  (текущая = F, merge commit)
         \         /
          C-------D
```

**Правило Copycara:** всегда используй `git merge --no-ff`. Это гарантирует создание
merge-коммита, и post-merge хук Copycara корректно создаст теневой коммит.

```bash
git merge --no-ff feat/int-conv2d-1x1   # ✅ правильно
git merge feat/int-conv2d-1x1           # ❌ может сделать fast-forward
```

---

## 2. Как работает Copycara

### 2.1. Две плоскости

Copycara хранит код в двух версиях:

| Плоскость | Где | Что содержит | Куда пушится |
|-----------|-----|-------------|-------------|
| **Dirty** (грязная) | Твоя рабочая директория | Код с комментариями, TODO, GRACE-тегами | `private` |
| **Clean** (чистая / shadow) | `.copycara/mirror` (скрытая папка) | Код БЕЗ комментариев | `origin`, `local` |

### 2.2. Shadow refs (теневые ссылки)

Каждая dirty-ветка имеет свою «теневую» копию. Например:
- `refs/heads/feat/integerization` — твоя грязная ветка
- `refs/copycara/heads/feat/integerization` — её чистая (shadow) копия

Shadow ref — это ЛОКАЛЬНАЯ git-ссылка. Она **не пушится** в публичные remote'ы
как `refs/copycara/heads/*`. Вместо этого `copycara push` пушит её как
`refs/heads/<ветка>` (обычную ветку).

### 2.3. Git hooks (хуки)

Copycara устанавливает 5 хуков в `.git/hooks/`:

| Хук | Когда срабатывает | Что делает |
|-----|------------------|-----------|
| post-commit | После `git commit` | Создаёт shadow-коммит (чистый) |
| post-merge | После `git merge` | То же самое |
| post-rewrite | После `git commit --amend` / `git rebase` | Пересоздаёт shadow-коммит |
| pre-push | Перед `git push <remote>` | Блокирует прямой пуш грязной ветки в public remote |
| post-checkout | После `git checkout` | Настраивает upstream на private |

### 2.4. `copycara push` — что происходит под капотом

```bash
copycara push
```

1. Определяет текущую ветку (например, `feat/integerization`)
2. Пушит `refs/copycara/heads/feat/integerization:refs/heads/feat/integerization` в каждый **public** remote (`origin`, `local`) — чистый код
3. Пушит `refs/heads/feat/integerization:refs/heads/feat/integerization` в каждый **private** remote — грязный код с комментариями
4. Пушит `refs/notes/copycara-map` в private — карту соответствия dirty↔clean

### 2.5. `copycara init` — что происходит

1. Создаёт `.copycara/` и shadow worktree (`.copycara/mirror`)
2. Создаёт `.copycara/config.toml` — настройки (не перезаписывает существующий!)
3. Создаёт `.copycara/.ignore` — файлы, исключаемые из public
4. Настраивает Git refspecs (правила маршрутизации push)
5. Устанавливает хуки
6. Создаёт начальный shadow-коммит для текущей ветки

При повторном запуске (`copycara init` на существующем проекте):
- Обновляет refspecs и хуки (исправляет расхождения с config.toml)
- **Не трогает** config.toml и .ignore
- Не пересоздаёт shadow-коммит, если он уже есть

---

## 3. Ежедневный workflow

### 3.1. Обычная работа

```bash
# Изменил код → закоммитил (хук сам создаст shadow-коммит)
git add .
git commit -m "feat: добавил новую фичу"

# Отправил изменения (во все remote одной командой)
copycara push
```

### 3.2. Получить изменения от коллег

```bash
copycara sync       # вместо git pull
```

### 3.3. Исправить последний коммит

```bash
git commit --amend -m "fix: обновлённое сообщение"
copycara push --force     # после amend нужен force
```

### 3.4. Создать новую ветку

```bash
git checkout -b feat/my-feature
copycara init               # создать shadow ref для новой ветки
# Работаешь, коммитишь...
copycara push               # первый пуш новой ветки
```

### 3.5. Скачать ветку, созданную на другом компьютере

```bash
# Скачать все ветки из private
git fetch private

# Создать локальную ветку на основе private/feat/something
git checkout -b feat/something private/feat/something

# Инициализировать shadow ref
copycara init

# Теперь можно пушить
copycara push --force        # первый раз нужен force
```

### 3.6. Слить две ветки

```bash
git checkout feat/target
git merge --no-ff feat/source     # ВАЖНО: --no-ff
copycara push
```

---

## 4. Типичные ошибки и их решение

### 4.1. «Push rejected — no common ancestor»

```
Error: Push rejected — the shadow ref for 'feat/integerization'
has no common ancestor with origin.
Fix: copycara push --force
```

**Причина:** После `copycara init` на ветке, которая уже существует на origin,
shadow-коммит — сирота (orphan). У него нет общего предка с веткой на origin.

**Решение:** `copycara push --force`. После этого force-пуша дальнейшие push'и
работают без force.

### 4.2. «src refspec не соответствует»

```
error: src refspec refs/copycara/heads/feat/... не соответствует
```

**Причина:** Shadow ref не существует. `copycara init` не был запущен на этой ветке.

**Решение:** `copycara init`, затем `copycara push --force`.

### 4.3. «matched multiple remote tracking branches»

```
fatal: 'feat/integerization' matched multiple (2) remote tracking branches
```

**Причина:** Ветка с таким именем есть на нескольких remote (origin + private).

**Решение:** Укажи remote явно:
```bash
git checkout -b feat/integerization private/feat/integerization
```

### 4.4. После merge не создался shadow-коммит

**Причина:** Fast-forward merge не создаёт новый коммит → post-merge хук не
срабатывает для shadow ref'а целевой ветки.

**Решение:** Всегда используй `git merge --no-ff`. Если уже сделал fast-forward:
```bash
copycara init       # пересоздаст shadow ref
copycara push --force
```

### 4.5. «copycara-map notes rejected on private»

```
[Warning] copycara-map notes rejected on private, retrying branch only...
```

**Причина:** Два разработчика создали разные notes-маппинги, и Git не может
выполнить fast-forward для notes.

**Решение:** Это предупреждение, а не ошибка. Dirty-ветка всё равно успешно
пушится. Notes будут синхронизированы при следующем push после того, как ты
сделаешь `git fetch private`.

---

## 5. Copycara и несколько компьютеров

### 5.1. Основной компьютер (synaptic)

На основном компьютере ты создал проект, настроил Copycara, работаешь.

### 5.2. Второй компьютер (Legion)

```bash
# 1. Клонируй с private (там грязный код с комментариями)
git clone git@github.com:vovasilenko/ai-codec.git
cd ai-codec

# 2. Настрой remote'ы (как на основном компьютере)
git remote add origin git@git.flight-control.ru:video-codecs/ai-codec.git
git remote add local git@gitlab.amulet-s.ru:vasilenko/nvc-rt.git
git remote rename origin private
git remote add origin git@git.flight-control.ru:video-codecs/ai-codec.git

# 3. Инициализируй Copycara
copycara init

# 4. Для каждой ветки, с которой работаешь:
git fetch private
git checkout -b feat/something private/feat/something
copycara init
copycara push --force     # только первый раз
```

### 5.3. Правило для нескольких компьютеров

**Перед началом работы на любом компьютере:**

```bash
git fetch private           # получить изменения с других компьютеров
copycara status             # проверить здоровье
```

**После завершения работы на любом компьютере:**

```bash
copycara push               # отправить ВСЁ (public + private)
```

---

## 6. Чек-лист «я не понимаю, что происходит»

Выполни эти шаги по порядку. Это решит 95% проблем.

```bash
# 1. Где я?
git status
git branch

# 2. Какие remote'ы?
git remote -v

# 3. Copycara жива?
copycara status

# 4. Если shadow ref отсутствует:
copycara init

# 5. Если ветка не появляется локально:
git fetch private
git checkout -b <имя-ветки> private/<имя-ветки>
copycara init

# 6. Пуш (первый раз с force):
copycara push --force

# 7. Если совсем всё сломалось:
copycara uninstall
copycara init
copycara push --force
```

---

## 7. Глоссарий

| Термин | Значение |
|--------|---------|
| **Remote** | Удалённый репозиторий (origin, local, private) |
| **Dirty / грязный код** | Код с комментариями, как ты его пишешь |
| **Clean / чистый код** | Код без комментариев, после обработки DLP |
| **Shadow ref** | Локальная git-ссылка `refs/copycara/heads/<ветка>` — чистая копия ветки |
| **Shadow commit** | Коммит в `.copycara/mirror` — чистая версия dirty-коммита |
| **Refspec** | Правило Git, которое говорит «при пуше подмени эту ссылку на ту» |
| **Fast-forward merge** | Слияние без создания нового коммита (Copycara не любит) |
| **Merge commit** | Слияние с явным новым коммитом (используй `--no-ff`) |
| **Upstream** | Ветка на remote, которую Git считает «главной» для локальной ветки |
| **Notes** | Git-объект `refs/notes/copycara-map` — хранит соответствие dirty↔clean |
