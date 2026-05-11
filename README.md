# Copycara: Topological Git DLP Engine

**Copycara** — это локальный Git-движок Data Loss Prevention (DLP), построенный на принципах **Quadrant Architecture**. Он позволяет разработчику прозрачно вести разработку с использованием приватных методологий (PACM, GRACE, семантические якоря), автоматически вырезая их перед отправкой в публичный репозиторий, но сохраняя полную топологию графа и приватный бэкап.

В основе движка лежит абстрактное синтаксическое дерево (AST) на базе библиотеки `uncomment`, что гарантирует 100% безопасную очистку исходного кода без повреждения синтаксиса (в отличие от регулярных выражений).

## 🚀 Ключевые возможности

* **Forward Smudge (Надстройка над Git Hooks):** Автоматически перехватывает `commit`, `merge` и `amend`, создает очищенную теневую копию в `.copycara/mirror` и связывает графы через `git notes`.
* **Reverse Smudge (Обратная синхронизация):** Команда `copycara sync` безопасно затягивает чистый код от других контрибьюторов в ваш размеченный workspace через алгоритм `3-way merge`.
* **State Machine для конфликтов:** Встроенный конечный автомат элегантно ставит процесс на паузу при `merge conflicts`, позволяя разрешить их руками и продолжить через `--continue`.
* **Идемпотентность:** Корректно обрабатывает перезапись истории (`rebase`, `amend`) без дублирования коммитов.

---

## 🛠 Установка

Убедитесь, что у вас установлен Rust (версии 1.70+).

```bash
# Клонирование и сборка монолитного бинарника
git clone <your-repo>/copycara-mcp
cd copycara-mcp
cargo build --release

# Опционально: добавление в PATH для глобального использования
sudo cp target/release/copycara-mcp /usr/local/bin/copycara
```

---

## 📖 Руководство пользователя (Workflow)

Движок делит мир на два репозитория: `origin` (публичный/чистый) и `private` (бэкап с разметкой).

### 1. Инициализация

Перейдите в ваш рабочий каталог с настроенными `origin` и `private` remote-серверами и выполните:

```bash
copycara init
```

Утилита создаст теневое дерево `.copycara/mirror`, пропишет нужный роутинг в `.git/config` и установит триггеры (`post-commit`, `post-rewrite`).

### 2. Ежедневная разработка

Работайте как обычно! Пишите код, оставляйте приватные маркеры `// DLP-DROP` или методологические комментарии, делайте коммиты.

```bash
git add .
git commit -m "feat: new feature with GRACE methodology"
git push origin   # Улетает очищенная теневая копия
git push private  # Улетает оригинальный код с вашими секретами
```

### 3. Синхронизация с сервером (Pull)

**Никогда не используйте `git pull`!** Это смешает грязный и чистый графы. Вместо этого используйте:

```bash
copycara sync
```

Утилита скачает чистый код, вычислит AST-безопасный патч и применит его поверх ваших секретов.

Если возникнет конфликт слияния:

1. Решите конфликт в редакторе (уберите маркеры `<<<<`, `====`, `>>>>`).
2. Добавьте файл в индекс: `git add <file>`.
3. Завершите синхронизацию: `copycara sync --continue`.

---

## 🧪 Инструкция по тестированию (Стресс-тест архитектуры)

Чтобы убедиться в надежности движка, вы можете развернуть локальную "песочницу" и прогнать 5 ключевых этапов жизненного цикла.

### Шаг 0: Подготовка песочницы

Выполните этот скрипт, чтобы создать тестовое окружение:

```bash
mkdir copycara-sandbox && cd copycara-sandbox
git init --bare public.git
git init --bare private.git
git clone public.git workspace
cd workspace
git remote add private ../private.git
git commit --allow-empty -m "init"

# Инициализируем движок (укажите путь до вашего бинарника copycara)
copycara init
```

### Этап 1: Базовая фильтрация и пустые коммиты

Проверяем, что AST-парсер удаляет комментарии, а движок игнорирует коммиты, состоящие только из секретов.

```bash
# Коммит с кодом и секретами
echo 'print("System initialized")' > main.py
echo '# TODO: добавить интеграцию с БД' >> main.py
git add main.py
git commit -m "feat: add main module"

# Коммит ТОЛЬКО с секретами (будет отброшен)
echo '    # Внимание: костыль // DLP-DROP' >> main.py
git add main.py
git commit -m "chore: private notes"

git push origin && git push private
# Проверка: git --git-dir=../public.git show HEAD:main.py (должен быть без секретов)
```

### Этап 2: Топология графов

Проверяем корректную работу с ветвлениями и merge-коммитами.

```bash
git checkout -b feature/auth
echo 'def auth(): return True' > auth.py
git add auth.py
git commit -m "feat: add auth logic"

git checkout main
git merge feature/auth --no-ff -m "Merge branch feature/auth"
git push origin && git push private
# Проверка: git --git-dir=../public.git log --graph --oneline (ромб должен сохраниться)
```

### Этап 3: Чистая обратная синхронизация

Эмулируем работу коллеги в чистом репозитории.

```bash
cd ..
git clone public.git coworker-workspace
cd coworker-workspace
echo 'def logout(): return False' >> auth.py
git add auth.py
git commit -m "feat: add logout logic"
git push origin main

# Возвращаемся в наш workspace и затягиваем изменения
cd ../workspace
copycara sync
```

### Этап 4: Разрешение конфликтов (State Machine)

Эмулируем пересечение изменений.

```bash
cd ../coworker-workspace
sed -i 's/System initialized/System online/g' main.py
git add main.py
git commit -m "fix: update init message"
git push origin main

cd ../workspace
copycara sync
# УТИЛИТА УПАДЕТ С ОШИБКОЙ КОНФЛИКТА - это норма!

# Разрешаем конфликт (в реальности вы отредактируете файл руками):
echo 'print("System online")' > main.py
echo '# TODO: добавить интеграцию с БД' >> main.py
echo '    # Внимание: костыль // DLP-DROP' >> main.py

git add main.py
copycara sync --continue
```

### Этап 5: Перезапись истории (Post-Rewrite)

Проверяем работу хука при `git commit --amend`.

```bash
echo 'print("End of program")' >> main.py
git add main.py
git commit -m "chore: finish program"

# Забыли секрет! Переписываем коммит:
echo '# Финальный костыль // DLP-DROP' >> main.py
git add main.py
git commit --amend -m "chore: finish program with fixes"

git push origin && git push private
```
