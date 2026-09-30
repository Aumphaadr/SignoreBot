# Сторонние компоненты и их лицензии

SignoreBot распространяется по лицензии [MIT](LICENSE) (неофициальный перевод —
[LICENSE_RU.md](LICENSE_RU.md)). Ниже по разделам — что в нём взято со стороны,
кто автор и на каких условиях. Шрифты, значки и библиотеки разрешают свободное
использование, в том числе в коммерческих продуктах; чужие картинки на сайте
описаны отдельно, в разделе «Сайт».

Панель, страницы оверлея и сайт ничего не грузят с других сайтов — ни с CDN, ни с
GitHub, в том числе с сайта и из репозитория набора Klaarheid: шрифты и значки лежат
в репозитории SignoreBot, это проверяет `npm run check`. Само приложение ходит в
сеть только к Twitch (API и чат), к OBS на этом компьютере и — для проверки
обновлений — за файлом `version.json` на сайт SignoreBot (или по адресу, который
вы указали в настройках).

## Шрифты

Все шрифты распространяются по лицензии **SIL Open Font License 1.1** — свободной
лицензии, разработанной специально для шрифтов. Она разрешает использование,
изучение, изменение и распространение (в том числе в составе программ) при
сохранении файла лицензии; продавать сами шрифты отдельно от программы нельзя.
Шрифты встроены как есть, без изменений.

| Шрифт | Где | Правообладатель | Лицензия |
|---|---|---|---|
| Jost | интерфейс панели и сайт | The Jost Project Authors | [src/assets/fonts/Jost-OFL.txt](src/assets/fonts/Jost-OFL.txt), копия — [docs/fonts/Jost-OFL.txt](docs/fonts/Jost-OFL.txt) |
| Source Code Pro | логи, переменные и адреса в панели | Adobe (Reserved Font Name «Source») | [src/assets/fonts/SourceCodePro-OFL.txt](src/assets/fonts/SourceCodePro-OFL.txt), [COPYRIGHT](src/assets/fonts/SourceCodePro-COPYRIGHT.txt) |
| Inter | текст на оверлее | The Inter Project Authors | [src-tauri/fonts/Inter-OFL.txt](src-tauri/fonts/Inter-OFL.txt) |
| Roboto | текст на оверлее | The Roboto Project Authors | [src-tauri/fonts/Roboto-OFL.txt](src-tauri/fonts/Roboto-OFL.txt) |
| Montserrat | текст на оверлее | The Montserrat Project Authors | [src-tauri/fonts/Montserrat-OFL.txt](src-tauri/fonts/Montserrat-OFL.txt) |
| Jost | текст на оверлее | The Jost Project Authors | [src-tauri/fonts/Jost-OFL.txt](src-tauri/fonts/Jost-OFL.txt) |
| Noto Sans Display | текст на оверлее | Google Inc. | [src-tauri/fonts/NotoSansDisplay-OFL.txt](src-tauri/fonts/NotoSansDisplay-OFL.txt) |
| Noto Serif | текст на оверлее | The Noto Project Authors | [src-tauri/fonts/NotoSerif-OFL.txt](src-tauri/fonts/NotoSerif-OFL.txt) |
| Oswald | текст на оверлее | The Oswald Project Authors | [src-tauri/fonts/Oswald-OFL.txt](src-tauri/fonts/Oswald-OFL.txt) |
| Comfortaa | текст на оверлее | The Comfortaa Project Authors (Reserved Font Name «Comfortaa») | [src-tauri/fonts/Comfortaa-OFL.txt](src-tauri/fonts/Comfortaa-OFL.txt) |
| Bellota | текст на оверлее | The Bellota Project Authors | [src-tauri/fonts/Bellota-OFL.txt](src-tauri/fonts/Bellota-OFL.txt) |
| Comic Relief | текст на оверлее | The Comic Relief Project Authors | [src-tauri/fonts/ComicRelief-OFL.txt](src-tauri/fonts/ComicRelief-OFL.txt) |
| Lobster | текст на оверлее | The Lobster Project Authors (Reserved Font Name «Lobster») | [src-tauri/fonts/Lobster-OFL.txt](src-tauri/fonts/Lobster-OFL.txt) |
| Neucha | текст на оверлее | Jovanny Lemonad | [src-tauri/fonts/Neucha-OFL.txt](src-tauri/fonts/Neucha-OFL.txt) |
| Handjet | текст на оверлее | The Handjet Project Authors | [src-tauri/fonts/Handjet-OFL.txt](src-tauri/fonts/Handjet-OFL.txt) |
| Rubik Mono One | текст на оверлее | Hubert and Fischer | [src-tauri/fonts/RubikMonoOne-OFL.txt](src-tauri/fonts/RubikMonoOne-OFL.txt) |

Шрифты для текста на оверлее встроены в приложение и отдаются странице оверлея с
локального сервера бота; список с лицензиями — `src-tauri/fonts/manifest.json`.

## Значки

<!-- klaarheid:start -->
Значки панели и сайта взяты из набора
[Klaarheid Icons](https://aumphaadr.github.io/Klaarheid-Icons/) ([репозиторий](https://github.com/Aumphaadr/Klaarheid-Icons)),
который разработан автором SignoreBot и опубликован под лицензией **MIT-0**.
Лицензия разрешает любое использование без условий и без упоминания автора.
В SignoreBot входят 125 значков набора в варианте «контур заливкой»; они лежат
в `src/assets/icons/klaarheid/` — это копии файлов набора байт в байт.

Раздел пересобирается командой `npm run icons:sync`.
<!-- klaarheid:end -->

Знаки Bits и баллов канала (`src/assets/icons/twitch/`) построены для SignoreBot тем же
способом — из отрезков и дуг окружностей на сетке 24 — и в набор не входят: это
обозначения функций Twitch (см. раздел «Twitch»). Как рисунки они распространяются на
условиях лицензии проекта (MIT).

## Логотип и значок приложения

Логотип (`src/assets/logo.svg`, `docs/logo.svg`), значок приложения (`src-tauri/icons/`,
`public/favicon.svg`) и иллюстрация на главной странице сайта (`docs/img/hero.jpg`)
сгенерированы с помощью ИИ и доработаны вручную автором проекта; распространяются на
условиях лицензии проекта (MIT).

## Сайт

- Снимки экрана SignoreBot в уроках (`docs/img/`) сделаны для проекта (MIT).
- Снимки окон OBS Studio и страниц Twitch в уроках (`docs/img/overlays-obs-*.png`,
  `periodic-obs.png`, `tips-obs-websocket.png`, `auth-twitch-*.png`) показывают чужие
  интерфейсы и приведены как иллюстрации к инструкциям. Права на эти интерфейсы — у
  их правообладателей (OBS Project, Twitch Interactive, Inc.); лицензия MIT на них не
  распространяется.
- Аватары в разделе «Спасибо» (`docs/img/thanks/`) — картинки профилей Twitch тех, кого
  благодарит проект. Права на них — у владельцев каналов; лицензия MIT на них не
  распространяется.

## Библиотеки

<!-- deps:start -->

### Панель (npm)

В панель при сборке попадают только эти пакеты; средства разработки (TypeScript, Vite,
svgo и другие из `devDependencies`) к пользователю не уходят.

| Пакет | Версия | Зачем | Лицензия |
|---|---|---|---|
| @tauri-apps/api | 2.11.1 | мост панели к ядру | Apache-2.0 OR MIT |
| @tauri-apps/plugin-clipboard-manager | 2.3.3 | буфер обмена | Apache-2.0 OR MIT |
| @tauri-apps/plugin-dialog | 2.7.3 | окна выбора файлов | Apache-2.0 OR MIT |
| @tauri-apps/plugin-opener | 2.5.5 | ссылки в браузере | Apache-2.0 OR MIT |
| react | 19.2.8 | интерфейс панели | MIT |
| react-dom | 19.2.8 | интерфейс панели | MIT |
| scheduler | 0.27.0 | приходит с ReactDOM | MIT |

### Ядро (Rust)

В бинарник SignoreBot под Linux и Windows линкуются 362 крейта. Прямые зависимости
проекта (остальные приходят с ними):

| Крейт | Версия | Лицензия |
|---|---|---|
| aho-corasick | 1.1.5 | MIT OR Unlicense |
| anyhow | 1.0.104 | Apache-2.0 OR MIT |
| axum | 0.8.9 | MIT |
| chrono | 0.4.45 | Apache-2.0 OR MIT |
| futures-util | 0.3.34 | Apache-2.0 OR MIT |
| infer | 0.16.0 | MIT |
| keyring | 3.6.3 | Apache-2.0 OR MIT |
| local-ip-address | 0.6.13 | Apache-2.0 OR MIT |
| mime | 0.3.17 | Apache-2.0 OR MIT |
| mime_guess | 2.0.5 | MIT |
| obws | 0.14.0 | MIT |
| open | 5.4.3 | MIT |
| parking_lot | 0.12.5 | Apache-2.0 OR MIT |
| rand | 0.8.8 | Apache-2.0 OR MIT |
| regex | 1.13.1 | Apache-2.0 OR MIT |
| reqwest | 0.12.28 | Apache-2.0 OR MIT |
| serde | 1.0.229 | Apache-2.0 OR MIT |
| serde_json | 1.0.151 | Apache-2.0 OR MIT |
| tauri | 2.11.5 | Apache-2.0 OR MIT |
| tauri-plugin-clipboard-manager | 2.3.3 | Apache-2.0 OR MIT |
| tauri-plugin-dialog | 2.7.3 | Apache-2.0 OR MIT |
| tauri-plugin-notification | 2.4.0 | Apache-2.0 OR MIT |
| tauri-plugin-opener | 2.5.5 | Apache-2.0 OR MIT |
| tauri-plugin-single-instance | 2.4.4 | Apache-2.0 OR MIT |
| thiserror | 2.0.20 | Apache-2.0 OR MIT |
| tokio | 1.53.1 | MIT |
| tokio-tungstenite | 0.26.2 | MIT |
| tokio-util | 0.7.19 | MIT |
| tower | 0.5.3 | MIT |
| tower-http | 0.6.11 | MIT |
| tracing | 0.1.44 | MIT |
| tracing-subscriber | 0.3.23 | MIT |
| ts-rs | 10.1.0 | MIT |
| url | 2.5.8 | Apache-2.0 OR MIT |
| uuid | 1.26.0 | Apache-2.0 OR MIT |

Все крейты по лицензиям (выражения SPDX; «A OR B» — можно выбрать любую):

| Лицензия | Крейтов |
|---|---|
| Apache-2.0 OR MIT | 210 |
| MIT | 94 |
| Unicode-3.0 | 15 |
| MIT OR Unlicense | 7 |
| Apache-2.0 OR MIT OR Zlib | 4 |
| BSD-3-Clause | 4 |
| Apache-2.0 | 3 |
| ISC | 3 |
| Apache-2.0 OR Apache-2.0 WITH LLVM-exception OR MIT | 2 |
| Apache-2.0 OR BSD-3-Clause | 2 |
| Apache-2.0 OR ISC OR MIT | 2 |
| BSD-3-Clause AND MIT | 2 |
| BSL-1.0 | 2 |
| CDLA-Permissive-2.0 | 2 |
| (Apache-2.0 OR MIT) AND Unicode-3.0 | 1 |
| 0BSD OR Apache-2.0 OR MIT | 1 |
| Apache-2.0 AND ISC | 1 |
| Apache-2.0 AND MIT | 1 |
| Apache-2.0 OR BSD-2-Clause OR MIT | 1 |
| Apache-2.0 OR BSL-1.0 | 1 |
| Apache-2.0 OR CC0-1.0 OR MIT-0 | 1 |
| BSD-3-Clause OR MIT | 1 |
| MPL-2.0 | 1 |
| Zlib | 1 |

Под MPL-2.0 — option-ext 0.2.0 (приходит с Tauri через dirs — пути к системным папкам).
MPL — слабый копилефт по файлам: такие крейты используются без изменений, их исходники открыты на crates.io.

Полный список с версиями — [third-party/rust-crates.md](third-party/rust-crates.md).

<!-- deps:end -->

Тексты лицензий лежат в исходниках каждого пакета и крейта (npm, crates.io).

## Twitch

SignoreBot работает через публичный API Twitch и подчиняется
[Twitch Developer Agreement](https://legal.twitch.com/legal/developer-agreement/); проект
не связан с Twitch и не одобрен им. Twitch — товарный знак Twitch Interactive, Inc.
Названия функций Twitch (Bits, баллы канала, хайповоз и другие) используются только для
того, чтобы их назвать. Client ID приложения открыт по умолчанию; форк вправе подставить
свой.

## Код и тексты

Код, тесты и документация (README, уроки сайта) созданы для SignoreBot и
распространяются по лицензии проекта (MIT). Они написаны с Claude — ИИ-ассистентом
Anthropic — под руководством автора.

## Если здесь что-то появится

Сторонний файл добавляется только вместе со своей лицензией и строкой в этот файл:
что это, откуда, кто автор, какая лицензия и где файл лежит. Порядок — в
[CONTRIBUTING.md](CONTRIBUTING.md#чужие-файлы).
