# Публикация релизов и автообновление

Код лежит в приватном `coddyxdev/Nimbus-Launcher`, а собранные сборки
публикуются в **публичный** `coddyxdev/Nimbus-Launcher-releases`.

Так сделано потому, что апдейтер скачивает `latest.json` и инсталлятор **без
авторизации**, а вложения релизов приватного репозитория отдают анонимным
запросам 404. Зашить токен в клиент нельзя — он окажется у всех пользователей.

## Текущая конфигурация

| Что | Значение |
| --- | --- |
| Репозиторий с кодом | `coddyxdev/Nimbus-Launcher` (приватный) |
| Репозиторий с релизами | `coddyxdev/Nimbus-Launcher-releases` (**должен быть публичным**) |
| Endpoint | `https://github.com/coddyxdev/Nimbus-Launcher-releases/releases/latest/download/latest.json` |
| Публичный ключ | прописан в `src-tauri/tauri.conf.json` → `plugins.updater.pubkey` |
| Идентификатор ключа | `2A5A244088956781` |
| Приватный ключ | `%USERPROFILE%\.tauri\nimbus.key` (зашифрован паролем) |

## 1. Создайте публичный репозиторий для релизов

На GitHub: **New repository** → имя `Nimbus-Launcher-releases` → **Public** →
поставьте галочку «Add a README file» и создайте.

README обязателен по двум причинам: в репозитории должна существовать ветка
`main` (workflow создаёт тег именно на ней), и на этот адрес можно ссылаться в
заявке на проверку Azure-приложения.

## 2. Создайте токен доступа

Workflow публикует релиз в другой репозиторий, поэтому встроенного
`GITHUB_TOKEN` недостаточно — у него нет прав за пределами своего репозитория.

GitHub → Settings (профиля) → Developer settings → Personal access tokens →
**Fine-grained tokens** → Generate new token:

| Поле | Значение |
| --- | --- |
| Repository access | Only select repositories → `Nimbus-Launcher-releases` |
| Permissions → Contents | **Read and write** |
| Expiration | на ваш выбор; после истечения релизы перестанут публиковаться |

## 3. Добавьте секреты в репозиторий с кодом

`Nimbus-Launcher` → Settings → Secrets and variables → Actions →
New repository secret:

| Имя секрета | Значение |
| --- | --- |
| `RELEASES_TOKEN` | токен из шага 2 |
| `TAURI_SIGNING_PRIVATE_KEY` | всё содержимое файла `%USERPROFILE%\.tauri\nimbus.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | пароль, заданный при генерации ключа |

Приватный ключ существует в единственном экземпляре: **потеряете — не сможете
выпускать обновления для уже установленных копий.** Сделайте резервную копию в
менеджере паролей.

## 4. Выпустите релиз

Версия берётся из `src-tauri/tauri.conf.json` (`version`) — поднимите её перед
тегом, иначе установленная копия не увидит обновление.

```bat
cd /d "C:\AIWorkspace\Nimbus Launcher"
git add -A
git commit -m "Release v1.7.0"
git tag v1.7.0
git push origin main --tags
```

Тег `v*` запускает `.github/workflows/release.yml`: сборка NSIS-инсталлятора,
подпись ключом, создание **черновика** релиза в публичном репозитории.

## 5. Опубликуйте черновик

`Nimbus-Launcher-releases` → Releases → откройте черновик → проверьте вложения →
**Publish release**.

В релизе должны быть три файла:

- `Nimbus.Client_1.7.0_x64-setup.exe` — обычный инсталлятор;
- `Nimbus.Client_1.7.0_x64-setup.nsis.zip` — то, что скачивает апдейтер;
- `latest.json` — манифест с версией, ссылкой и подписью.

Пока релиз в черновиках, `releases/latest/download/...` его не отдаёт —
обновление придёт только после публикации.

## Проверка

Установите предыдущую версию, запустите её и включите режим разработчика
(Настройки → Обслуживание). В логе появится одна из записей:

| Запись | Значение |
| --- | --- |
| `updater: доступна версия 1.7.0` | всё работает, сверху появится плашка обновления |
| `updater: установлена последняя версия` | версии совпадают |
| `updater: не настроен` | в конфиге остались placeholder-ы `REPLACE_*` |
| `updater: проверка не удалась — ...` | сеть, репозиторий релизов приватный или `latest.json` отсутствует |

Быстрая проверка доступности вручную — откройте endpoint в браузере
в режиме инкогнито. Должен отдаться JSON, а не страница логина GitHub.

## Частые ошибки

| Ошибка в Actions | Причина |
| --- | --- |
| `invalid target_commitish` | в репозитории релизов нет ветки `main` — создайте README |
| `Resource not accessible by integration` | `RELEASES_TOKEN` не выдан или без прав Contents: Read and write |
| `signature verification failed` у клиента | сборка без `TAURI_SIGNING_PRIVATE_KEY`, либо пара ключей заменена после предыдущего релиза |
