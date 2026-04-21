use crossterm::style::Color;
use std::sync::OnceLock;

pub struct ColorScheme {
    pub name: &'static str,
    pub id: &'static str,
    pub primary_bg: Color,
    pub primary_fg: Color,
    pub accent_fg: Color,
    pub selection_bg: Color,
    pub error_bg: Color,
    pub summary_bg: Color,
    pub summary_border: Color,
    pub summary_accent: Color,
    pub summary_success: Color,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb { r, g, b }
}

pub const SCHEMES: &[ColorScheme] = &[
    ColorScheme {
        name: "Classic",
        id: "classic",
        primary_bg: rgb(0, 0, 170),
        primary_fg: rgb(255, 255, 255),
        accent_fg: rgb(170, 170, 0),
        selection_bg: rgb(0, 0, 0),
        error_bg: rgb(170, 0, 0),
        summary_bg: rgb(0, 0, 0),
        summary_border: rgb(0, 0, 170),
        summary_accent: rgb(170, 170, 0),
        summary_success: rgb(0, 170, 0),
    },
    ColorScheme {
        name: "Pastel",
        id: "pastel",
        primary_bg: rgb(220, 210, 240),
        primary_fg: rgb(60, 50, 90),
        accent_fg: rgb(200, 120, 140),
        selection_bg: rgb(255, 235, 215),
        error_bg: rgb(240, 180, 180),
        summary_bg: rgb(240, 235, 250),
        summary_border: rgb(160, 140, 200),
        summary_accent: rgb(200, 120, 140),
        summary_success: rgb(150, 200, 160),
    },
    ColorScheme {
        name: "Beige",
        id: "beige",
        primary_bg: rgb(210, 180, 140),
        primary_fg: rgb(60, 40, 20),
        accent_fg: rgb(180, 90, 40),
        selection_bg: rgb(240, 220, 180),
        error_bg: rgb(180, 70, 50),
        summary_bg: rgb(40, 30, 20),
        summary_border: rgb(180, 140, 90),
        summary_accent: rgb(220, 160, 80),
        summary_success: rgb(150, 160, 90),
    },
    ColorScheme {
        name: "House Stark",
        id: "stark",
        primary_bg: rgb(50, 55, 60),
        primary_fg: rgb(230, 235, 240),
        accent_fg: rgb(180, 190, 200),
        selection_bg: rgb(90, 95, 100),
        error_bg: rgb(100, 40, 40),
        summary_bg: rgb(20, 25, 30),
        summary_border: rgb(140, 150, 160),
        summary_accent: rgb(220, 220, 230),
        summary_success: rgb(160, 200, 180),
    },
    ColorScheme {
        name: "House Lannister",
        id: "lannister",
        primary_bg: rgb(130, 20, 30),
        primary_fg: rgb(255, 220, 100),
        accent_fg: rgb(255, 240, 180),
        selection_bg: rgb(90, 10, 20),
        error_bg: rgb(60, 10, 10),
        summary_bg: rgb(30, 5, 10),
        summary_border: rgb(180, 40, 40),
        summary_accent: rgb(255, 220, 100),
        summary_success: rgb(220, 180, 80),
    },
    ColorScheme {
        name: "House Tyrell",
        id: "tyrell",
        primary_bg: rgb(30, 90, 50),
        primary_fg: rgb(255, 230, 140),
        accent_fg: rgb(255, 200, 210),
        selection_bg: rgb(80, 130, 70),
        error_bg: rgb(130, 60, 30),
        summary_bg: rgb(15, 40, 25),
        summary_border: rgb(100, 170, 100),
        summary_accent: rgb(255, 220, 130),
        summary_success: rgb(200, 240, 160),
    },
    ColorScheme {
        name: "House Martell",
        id: "martell",
        primary_bg: rgb(180, 70, 20),
        primary_fg: rgb(255, 245, 220),
        accent_fg: rgb(255, 215, 80),
        selection_bg: rgb(90, 25, 5),
        error_bg: rgb(130, 20, 15),
        summary_bg: rgb(40, 15, 5),
        summary_border: rgb(235, 110, 40),
        summary_accent: rgb(255, 210, 90),
        summary_success: rgb(210, 230, 150),
    },
    ColorScheme {
        name: "House Greyjoy",
        id: "greyjoy",
        primary_bg: rgb(15, 15, 20),
        primary_fg: rgb(200, 170, 80),
        accent_fg: rgb(100, 140, 140),
        selection_bg: rgb(40, 45, 55),
        error_bg: rgb(80, 30, 40),
        summary_bg: rgb(5, 5, 10),
        summary_border: rgb(60, 100, 110),
        summary_accent: rgb(220, 180, 80),
        summary_success: rgb(120, 180, 160),
    },
    ColorScheme {
        name: "House Targaryen",
        id: "targaryen",
        primary_bg: rgb(10, 10, 10),
        primary_fg: rgb(220, 40, 40),
        accent_fg: rgb(255, 140, 60),
        selection_bg: rgb(70, 15, 15),
        error_bg: rgb(160, 20, 20),
        summary_bg: rgb(5, 0, 0),
        summary_border: rgb(210, 60, 60),
        summary_accent: rgb(255, 205, 80),
        summary_success: rgb(225, 225, 235),
    },
    ColorScheme {
        name: "House Baratheon",
        id: "baratheon",
        primary_bg: rgb(210, 170, 45),
        primary_fg: rgb(20, 20, 20),
        accent_fg: rgb(135, 20, 25),
        selection_bg: rgb(255, 215, 90),
        error_bg: rgb(185, 35, 35),
        summary_bg: rgb(20, 20, 20),
        summary_border: rgb(225, 180, 55),
        summary_accent: rgb(225, 55, 55),
        summary_success: rgb(95, 205, 105),
    },
    ColorScheme {
        name: "House Arryn",
        id: "arryn",
        primary_bg: rgb(100, 140, 200),
        primary_fg: rgb(245, 245, 250),
        accent_fg: rgb(220, 230, 240),
        selection_bg: rgb(60, 90, 160),
        error_bg: rgb(120, 40, 40),
        summary_bg: rgb(20, 30, 50),
        summary_border: rgb(130, 170, 220),
        summary_accent: rgb(240, 240, 250),
        summary_success: rgb(200, 220, 240),
    },
    ColorScheme {
        name: "House Tully",
        id: "tully",
        primary_bg: rgb(40, 60, 110),
        primary_fg: rgb(220, 220, 230),
        accent_fg: rgb(200, 60, 60),
        selection_bg: rgb(90, 30, 40),
        error_bg: rgb(140, 30, 30),
        summary_bg: rgb(20, 30, 50),
        summary_border: rgb(100, 130, 180),
        summary_accent: rgb(220, 80, 80),
        summary_success: rgb(180, 200, 220),
    },
];

pub fn default_scheme() -> &'static ColorScheme {
    SCHEMES
        .iter()
        .find(|s| s.id == "stark")
        .unwrap_or(&SCHEMES[0])
}

pub fn by_id(id: &str) -> &'static ColorScheme {
    SCHEMES
        .iter()
        .find(|s| s.id.eq_ignore_ascii_case(id))
        .unwrap_or_else(default_scheme)
}

static ACTIVE: OnceLock<std::sync::RwLock<&'static ColorScheme>> = OnceLock::new();

fn cell() -> &'static std::sync::RwLock<&'static ColorScheme> {
    ACTIVE.get_or_init(|| std::sync::RwLock::new(default_scheme()))
}

pub fn set_active(id: &str) {
    let scheme = by_id(id);
    if let Ok(mut guard) = cell().write() {
        *guard = scheme;
    }
}

pub fn active() -> &'static ColorScheme {
    *cell().read().expect("theme lock poisoned")
}
