# Сторонние компоненты и материалы

SignoreBot распространяется по лицензии MIT (см. `LICENSE`). Ниже — то, что
в него входит помимо собственного кода, и на каких условиях.

## Шрифты

Все — SIL Open Font License 1.1; тексты лицензий лежат рядом с файлами
(`src-tauri/fonts/*-OFL.txt`, `src/assets/fonts/OFL.txt`). Шрифты встроены как
есть, без изменений; продавать их отдельно OFL запрещает, а распространять
вместе с программой — разрешает.

- **Jost** (`src/assets/fonts/`, `docs/fonts/`) — интерфейс панели и сайт;
  © The Jost Project Authors.
- Шрифты для текста на оверлее (`src-tauri/fonts/`), встроены в приложение и
  отдаются странице оверлея с локального сервера: **Inter**, **Roboto**,
  **Montserrat**, **Jost**, **Noto Sans Display**, **Noto Serif**, **Oswald**,
  **Comfortaa**, **Bellota**, **Comic Relief**, **Lobster**, **Neucha**,
  **Handjet**, **Rubik Mono One** — авторские права у соответствующих
  авторов, см. заголовок каждого `OFL.txt`.

## Иконки и изображения

- Иконки интерфейса (`src/assets/icons/`), логотип и иллюстрации на сайте
  сгенерированы с помощью ИИ и доработаны вручную автором проекта.
  Распространяются вместе с проектом на условиях MIT.

## Библиотеки

- **Tauri 2** и его плагины — MIT или Apache-2.0.
- Крейты Rust (axum, tokio, reqwest, keyring, obws и остальные) — MIT,
  Apache-2.0, BSD, Zlib, Unicode-3.0; пять крейтов (cssparser, selectors и
  сопутствующие, приходят с Tauri) — MPL-2.0, используются без изменений.
- Пакеты npm (React, Vite, svgo и остальные) — MIT, ISC, Apache-2.0.

Полные тексты лицензий зависимостей — в исходниках соответствующих пакетов
(`cargo metadata`, `node_modules/*/LICENSE`).

## Twitch

Приложение работает через публичный Twitch API и подчиняется Twitch Developer
Agreement. Проект не аффилирован с Twitch. Client ID приложения открыт по
умолчанию; форк вправе подставить свой.
