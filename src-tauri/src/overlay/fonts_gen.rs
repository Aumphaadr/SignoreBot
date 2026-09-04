// СГЕНЕРИРОВАНО tools/build-fonts.mjs — не редактировать вручную.
/// Файлы шрифтов, встроенные в бинарник; отдаются оверлею по /fonts/<имя>.
pub static FONTS: &[(&str, &[u8])] = &[
    ("Inter.ttf", include_bytes!("../../fonts/Inter.ttf")),
    ("Roboto.ttf", include_bytes!("../../fonts/Roboto.ttf")),
    ("Montserrat.ttf", include_bytes!("../../fonts/Montserrat.ttf")),
    ("Jost.ttf", include_bytes!("../../fonts/Jost.ttf")),
    ("NotoSansDisplay.ttf", include_bytes!("../../fonts/NotoSansDisplay.ttf")),
    ("NotoSerif.ttf", include_bytes!("../../fonts/NotoSerif.ttf")),
    ("Oswald.ttf", include_bytes!("../../fonts/Oswald.ttf")),
    ("Comfortaa.ttf", include_bytes!("../../fonts/Comfortaa.ttf")),
    ("Bellota-Regular.ttf", include_bytes!("../../fonts/Bellota-Regular.ttf")),
    ("Bellota-Bold.ttf", include_bytes!("../../fonts/Bellota-Bold.ttf")),
    ("ComicRelief-Regular.ttf", include_bytes!("../../fonts/ComicRelief-Regular.ttf")),
    ("ComicRelief-Bold.ttf", include_bytes!("../../fonts/ComicRelief-Bold.ttf")),
    ("Lobster.ttf", include_bytes!("../../fonts/Lobster.ttf")),
    ("Neucha.ttf", include_bytes!("../../fonts/Neucha.ttf")),
    ("Handjet.ttf", include_bytes!("../../fonts/Handjet.ttf")),
    ("RubikMonoOne.ttf", include_bytes!("../../fonts/RubikMonoOne.ttf")),
];
/// @font-face для страницы оверлея (пути относительно сервера оверлеев).
pub const FONT_FACE_CSS: &str = "@font-face{font-family:\"Inter\";src:url(\"/fonts/Inter.ttf\") format(\"truetype\");font-weight:100 900;font-style:normal;font-display:swap}\n@font-face{font-family:\"Roboto\";src:url(\"/fonts/Roboto.ttf\") format(\"truetype\");font-weight:100 900;font-style:normal;font-display:swap}\n@font-face{font-family:\"Montserrat\";src:url(\"/fonts/Montserrat.ttf\") format(\"truetype\");font-weight:100 900;font-style:normal;font-display:swap}\n@font-face{font-family:\"Jost\";src:url(\"/fonts/Jost.ttf\") format(\"truetype\");font-weight:100 900;font-style:normal;font-display:swap}\n@font-face{font-family:\"Noto Sans Display\";src:url(\"/fonts/NotoSansDisplay.ttf\") format(\"truetype\");font-weight:100 900;font-style:normal;font-display:swap}\n@font-face{font-family:\"Noto Serif\";src:url(\"/fonts/NotoSerif.ttf\") format(\"truetype\");font-weight:100 900;font-style:normal;font-display:swap}\n@font-face{font-family:\"Oswald\";src:url(\"/fonts/Oswald.ttf\") format(\"truetype\");font-weight:200 700;font-style:normal;font-display:swap}\n@font-face{font-family:\"Comfortaa\";src:url(\"/fonts/Comfortaa.ttf\") format(\"truetype\");font-weight:300 700;font-style:normal;font-display:swap}\n@font-face{font-family:\"Bellota\";src:url(\"/fonts/Bellota-Regular.ttf\") format(\"truetype\");font-weight:400;font-style:normal;font-display:swap}\n@font-face{font-family:\"Bellota\";src:url(\"/fonts/Bellota-Bold.ttf\") format(\"truetype\");font-weight:700;font-style:normal;font-display:swap}\n@font-face{font-family:\"Comic Relief\";src:url(\"/fonts/ComicRelief-Regular.ttf\") format(\"truetype\");font-weight:400;font-style:normal;font-display:swap}\n@font-face{font-family:\"Comic Relief\";src:url(\"/fonts/ComicRelief-Bold.ttf\") format(\"truetype\");font-weight:700;font-style:normal;font-display:swap}\n@font-face{font-family:\"Lobster\";src:url(\"/fonts/Lobster.ttf\") format(\"truetype\");font-weight:400;font-style:normal;font-display:swap}\n@font-face{font-family:\"Neucha\";src:url(\"/fonts/Neucha.ttf\") format(\"truetype\");font-weight:400;font-style:normal;font-display:swap}\n@font-face{font-family:\"Handjet\";src:url(\"/fonts/Handjet.ttf\") format(\"truetype\");font-weight:100 900;font-style:normal;font-display:swap}\n@font-face{font-family:\"Rubik Mono One\";src:url(\"/fonts/RubikMonoOne.ttf\") format(\"truetype\");font-weight:400;font-style:normal;font-display:swap}";
