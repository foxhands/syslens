# Как SysLens попадает к людям

Все файлы собирает GitHub Actions (`.github/workflows/build.yml`). Метка вида
`v0.4.0` публикует их на странице Releases.

## Что публикуется

| Файл | Для чего |
| --- | --- |
| `syslens.exe` | Windows 10/11, 64-bit |
| `syslens-windows-arm64.exe` | Windows на ARM |
| `syslens-linux-x86_64.tar.gz`, `SysLens-x86_64.AppImage` | Linux x86_64 |
| `syslens-linux-arm64.tar.gz` | Linux ARM64 |
| `SysLens-macos.zip` | macOS, Intel и Apple Silicon (пробная сборка) |
| `syslens.json` | манифест Scoop |
| `winget-manifest.zip` | манифест winget |

AppImage содержит только саму программу: графические библиотеки (X11 или
Wayland, OpenGL) берутся из системы — в любом настольном дистрибутиве они есть.

## Scoop

Манифест создаётся при каждом выпуске и лежит рядом с программой:

```
scoop install https://github.com/foxhands/syslens/releases/latest/download/syslens.json
```

Чтобы программа ставилась короткой командой `scoop install syslens`, нужен свой
«bucket» — отдельный репозиторий с этим файлом — или заявка в общий bucket
`extras`.

## winget

В `winget-manifest.zip` лежат три готовых файла манифеста (`Foxhand.SysLens`).
В каталог winget программа попадает через заявку (pull request) в репозиторий
`microsoft/winget-pkgs`: файлы кладутся в папку
`manifests/f/Foxhand/SysLens/<версия>/`. Заявку подаёт автор программы — сама
она не отправляется. Проверить манифест у себя: `winget validate <папка>`.

## Подпись программы (Windows)

Сейчас программа не подписана, поэтому Windows SmartScreen при первом запуске
показывает предупреждение. Убрать его можно только сертификатом подписи кода —
его выдают за деньги центры сертификации (Certum, Sectigo, DigiCert и другие),
а для открытых проектов есть бесплатная программа SignPath Foundation.

В сборке уже есть шаг подписи; он включается сам, когда в настройках
репозитория (Settings → Secrets and variables → Actions) заданы два секрета:

- `SIGN_PFX_BASE64` — файл сертификата `.pfx`, закодированный в Base64;
- `SIGN_PFX_PASSWORD` — пароль к нему.

Без секретов шаг пропускается. Шаг написан заранее и на настоящем сертификате
ещё не проверялся. Сертификаты на аппаратном ключе (USB-токен, облачный HSM)
в виде `.pfx` не выгружаются — для них шаг придётся заменить на средство
подписи от того, кто выдал сертификат.

## macOS

Программа для macOS не подписана и не заверена Apple (для этого нужна платная
учётная запись разработчика). При первом запуске: правая кнопка по
`SysLens.app` → «Открыть».
