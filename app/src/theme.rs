//! Look and feel: palette, fonts, spacing; dark and light modes, and colour themes.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke, TextStyle, Theme, ThemePreference, Visuals};

/// Name of the semibold font family used for headings and titles.
pub const SEMIBOLD: &str = "semibold";

/// The type scale. Use these rather than literal sizes.
pub mod size {
    /// Captions, section labels, badges in tight spots.
    pub const XS: f32 = 11.5;
    /// Secondary lines: explanations under titles, hints.
    pub const SM: f32 = 12.5;
    /// Body text and buttons.
    pub const BODY: f32 = 14.0;
    /// Card titles.
    pub const MD: f32 = 15.5;
    /// Section titles.
    pub const LG: f32 = 17.0;
    /// Page titles.
    pub const XL: f32 = 22.0;
    /// Headline numbers in stat tiles.
    pub const XXL: f32 = 28.0;
    /// Hero numbers: one per page at most.
    #[allow(dead_code)] // part of the scale, unused for now
    pub const DISPLAY: f32 = 40.0;
}

/// Gaps between things, before the density setting scales them (see `space`).
#[allow(dead_code)] // the whole scale, though not every step is used yet
pub mod gap {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 24.0;
}

/// Corner radius of cards, panels and dialogs.
pub const RADIUS: u8 = 12;

/// Dark, light, or whatever Windows uses; saved as `theme` in settings.json.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    System,
    Light,
    Dark,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::System, Mode::Light, Mode::Dark];

    pub fn key(self) -> &'static str {
        match self {
            Mode::System => "system",
            Mode::Light => "light",
            Mode::Dark => "dark",
        }
    }

    /// Unknown or missing keys mean dark, the default before System existed.
    pub fn from_key(key: &str) -> Mode {
        Mode::ALL.into_iter().find(|m| m.key() == key).unwrap_or(Mode::Dark)
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::System => "◐ System",
            Mode::Light => "☀ Light",
            Mode::Dark => "🌙 Dark",
        }
    }

    pub fn preference(self) -> ThemePreference {
        match self {
            Mode::System => ThemePreference::System,
            Mode::Light => ThemePreference::Light,
            Mode::Dark => ThemePreference::Dark,
        }
    }
}

/// Where chibi Seiun Sky appears; saved as `chibi` in settings.json. She's only ever drawn
/// where nothing else is, so she never covers content.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Chibi {
    /// Home's welcome, empty pages, and small beside every page title.
    Everywhere,
    /// Home's welcome and empty pages (the default).
    Home,
    Off,
}

impl Chibi {
    pub const ALL: [Chibi; 3] = [Chibi::Everywhere, Chibi::Home, Chibi::Off];

    pub fn key(self) -> &'static str {
        match self {
            Chibi::Everywhere => "everywhere",
            Chibi::Home => "home",
            Chibi::Off => "off",
        }
    }

    pub fn from_key(key: &str) -> Option<Chibi> {
        Chibi::ALL.into_iter().find(|c| c.key() == key)
    }
}

static CHIBI: AtomicU8 = AtomicU8::new(1);

/// Where the mascot goes, app-wide (read by widgets that have no `App`).
pub fn chibi() -> Chibi {
    Chibi::ALL[usize::from(CHIBI.load(Ordering::Relaxed)).min(Chibi::ALL.len() - 1)]
}

pub fn use_chibi(c: Chibi) {
    CHIBI.store(Chibi::ALL.iter().position(|x| *x == c).unwrap_or(1) as u8, Ordering::Relaxed);
}

/// 0 = comfortable, 1 = compact (spacing × 0.8).
static DENSITY: AtomicU8 = AtomicU8::new(0);

/// How much spacing is scaled: 1.0 comfortable, 0.8 compact.
pub fn density() -> f32 {
    if DENSITY.load(Ordering::Relaxed) == 1 { 0.8 } else { 1.0 }
}

/// A gap from `gap`, scaled by the density setting.
pub fn space(g: f32) -> f32 {
    g * density()
}

/// Switch between comfortable and compact spacing; restyles a running app.
pub fn set_compact(ctx: &egui::Context, compact: bool) {
    DENSITY.store(u8::from(compact), Ordering::Relaxed);
    for theme in [Theme::Dark, Theme::Light] {
        ctx.style_mut_of(theme, apply_spacing);
    }
}

/// The UI scale range offered in Settings (`ui_scale` in settings.json).
pub const UI_SCALE: std::ops::RangeInclusive<f32> = 0.9..=1.25;

/// A colour theme: the accent (highlights, selections, main buttons), link colour, and the
/// background's sky and decorations. Each works in dark and light mode. Green stays for good news
/// in every theme, so none of them is green.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Look {
    Sky,
    Sakura,
    Sunset,
    Night,
    /// Turf brown and trophy gold, with a racetrack's rails.
    Track,
    /// High contrast: black and white, no decorations.
    Ink,
}

impl Look {
    pub const ALL: [Look; 6] = [Look::Sky, Look::Sakura, Look::Sunset, Look::Night, Look::Track, Look::Ink];

    pub fn label(self) -> &'static str {
        match self {
            Look::Sky => "Seiun Sky",
            Look::Sakura => "Sakura",
            Look::Sunset => "Sunset",
            Look::Night => "Starry night",
            Look::Track => "Track day",
            Look::Ink => "Ink",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Look::Sky => "Sky blue, with soft clouds",
            Look::Sakura => "Cherry-blossom pink, with falling petals",
            Look::Sunset => "Warm orange, with an evening sun",
            Look::Night => "Violet, with stars and a moon",
            Look::Track => "Trophy gold, with the rails of a racetrack",
            Look::Ink => "High contrast black and white, plain background",
        }
    }

    /// The name saved in settings.json.
    pub fn key(self) -> &'static str {
        match self {
            Look::Sky => "sky",
            Look::Sakura => "sakura",
            Look::Sunset => "sunset",
            Look::Night => "night",
            Look::Track => "track",
            Look::Ink => "ink",
        }
    }

    pub fn from_key(key: &str) -> Look {
        Look::ALL.into_iter().find(|l| l.key() == key).unwrap_or(Look::Sky)
    }
}

static LOOK: AtomicU8 = AtomicU8::new(0);

/// The current colour theme.
pub fn look() -> Look {
    Look::ALL[usize::from(LOOK.load(Ordering::Relaxed)).min(Look::ALL.len() - 1)]
}

/// Use a colour theme from now on; `set_look` also restyles a running app.
pub fn use_look(look: Look) {
    LOOK.store(Look::ALL.iter().position(|l| *l == look).unwrap_or(0) as u8, Ordering::Relaxed);
}

/// Switch the running app to a colour theme.
pub fn set_look(ctx: &egui::Context, look: Look) {
    use_look(look);
    for theme in [Theme::Dark, Theme::Light] {
        ctx.set_visuals_of(theme, visuals(theme == Theme::Dark));
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub surface: Color32,
    pub raised: Color32,
    pub hover: Color32,
    pub input: Color32,
    pub border: Color32,
    pub text: Color32,
    pub weak: Color32,
    /// Highlights, selections and main buttons: the colour of the theme's background.
    pub accent: Color32,
    /// Text on an accent-filled button.
    pub on_accent: Color32,
    /// The accent, darker: outlines of selected things in light mode.
    pub accent_strong: Color32,
    pub link: Color32,
    pub gold: Color32,
    pub pink: Color32,
    /// The background's colour toward the bottom right.
    pub sky: Color32,
    /// Good news only: gains, success, things you can get. Readable as text in both modes.
    pub good: Color32,
    /// Caution: not announced, below target, locked.
    pub warn: Color32,
    /// Errors and destructive actions.
    pub bad: Color32,
}

/// Accent, text on it, its strong version, link and sky colour: per theme, dark then light.
fn tint(look: Look, dark: bool) -> [Color32; 5] {
    let c = Color32::from_rgb;
    match (look, dark) {
        (Look::Sky, true) => [c(96, 165, 250), c(10, 20, 36), c(96, 165, 250), c(125, 211, 252), c(30, 52, 84)],
        (Look::Sky, false) => [c(14, 110, 190), Color32::WHITE, c(10, 85, 150), c(3, 105, 161), c(204, 228, 250)],
        (Look::Sakura, true) => [c(244, 143, 177), c(40, 10, 22), c(244, 143, 177), c(249, 168, 212), c(70, 34, 54)],
        (Look::Sakura, false) => [c(204, 52, 108), Color32::WHITE, c(160, 32, 82), c(176, 30, 94), c(252, 221, 233)],
        (Look::Sunset, true) => [c(251, 157, 75), c(40, 20, 6), c(251, 157, 75), c(253, 196, 128), c(86, 44, 34)],
        (Look::Sunset, false) => [c(180, 80, 10), Color32::WHITE, c(150, 66, 8), c(170, 78, 10), c(255, 222, 190)],
        (Look::Night, true) => [c(167, 139, 250), c(22, 14, 46), c(167, 139, 250), c(196, 181, 253), c(40, 32, 84)],
        (Look::Night, false) => [c(109, 58, 214), Color32::WHITE, c(84, 40, 170), c(92, 46, 190), c(226, 218, 252)],
        (Look::Track, true) => [c(234, 179, 8), c(30, 20, 0), c(234, 179, 8), c(250, 204, 21), c(58, 42, 20)],
        (Look::Track, false) => [c(146, 100, 0), Color32::WHITE, c(120, 80, 0), c(133, 77, 14), c(247, 234, 200)],
        (Look::Ink, true) => [c(236, 238, 242), c(12, 13, 16), c(236, 238, 242), c(191, 219, 254), c(30, 32, 38)],
        (Look::Ink, false) => [c(17, 19, 24), Color32::WHITE, c(0, 0, 0), c(29, 78, 216), c(226, 228, 232)],
    }
}

/// The palette for the current theme.
pub fn palette(dark: bool) -> Palette {
    palette_for(look(), dark)
}

pub fn palette_for(look: Look, dark: bool) -> Palette {
    let [accent, on_accent, accent_strong, link, sky] = tint(look, dark);
    let p = base_palette(dark, accent, on_accent, accent_strong, link, sky);
    if look != Look::Ink {
        return p;
    }
    // Ink: stronger secondary text and borders, for high contrast.
    if dark {
        Palette { bg: Color32::BLACK, surface: Color32::from_rgb(14, 15, 18), weak: Color32::from_rgb(196, 201, 210), border: Color32::from_rgb(92, 98, 110), ..p }
    } else {
        Palette { bg: Color32::WHITE, surface: Color32::WHITE, weak: Color32::from_rgb(52, 58, 68), border: Color32::from_rgb(120, 126, 136), ..p }
    }
}

fn base_palette(dark: bool, accent: Color32, on_accent: Color32, accent_strong: Color32, link: Color32, sky: Color32) -> Palette {
    if dark {
        Palette {
            bg: Color32::from_rgb(17, 19, 24),
            surface: Color32::from_rgb(25, 28, 34),
            raised: Color32::from_rgb(34, 38, 46),
            hover: Color32::from_rgb(43, 48, 58),
            input: Color32::from_rgb(12, 14, 18),
            border: Color32::from_rgb(44, 49, 59),
            text: Color32::from_rgb(231, 233, 238),
            weak: Color32::from_rgb(143, 151, 163),
            accent,
            on_accent,
            accent_strong,
            link,
            gold: Color32::from_rgb(250, 204, 21),
            pink: Color32::from_rgb(244, 114, 182),
            sky,
            good: Color32::from_rgb(34, 197, 94),
            warn: Color32::from_rgb(245, 158, 11),
            bad: Color32::from_rgb(248, 113, 113),
        }
    } else {
        Palette {
            bg: Color32::from_rgb(243, 244, 246),
            surface: Color32::WHITE,
            raised: Color32::from_rgb(236, 238, 242),
            hover: Color32::from_rgb(226, 229, 234),
            input: Color32::from_rgb(244, 245, 248),
            border: Color32::from_rgb(214, 218, 225),
            text: Color32::from_rgb(29, 33, 41),
            weak: Color32::from_rgb(93, 101, 114),
            accent,
            on_accent,
            accent_strong,
            link,
            // Darker than the dark-mode colours, so they read as text on white (WCAG AA).
            gold: Color32::from_rgb(146, 100, 0),
            pink: Color32::from_rgb(200, 30, 105),
            sky,
            good: Color32::from_rgb(21, 128, 61),
            warn: Color32::from_rgb(180, 83, 9),
            bad: Color32::from_rgb(185, 28, 28),
        }
    }
}

/// WCAG contrast ratio between two colours (1 to 21; text needs 4.5).
pub fn contrast(a: Color32, b: Color32) -> f32 {
    fn lum(c: Color32) -> f32 {
        let ch = |v: u8| {
            let v = f32::from(v) / 255.0;
            if v <= 0.039_28 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
    }
    let (la, lb) = (lum(a), lum(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// The palette for the theme `ui` is drawn in.
pub fn pal(ui: &egui::Ui) -> Palette {
    palette(ui.visuals().dark_mode)
}

fn visuals(dark: bool) -> Visuals {
    let p = palette(dark);
    let mut v = if dark { Visuals::dark() } else { Visuals::light() };
    // Pages are see-through: main.rs paints the background (and the sky) behind every panel.
    v.panel_fill = Color32::TRANSPARENT;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.input;
    // Table stripes tint what's behind them rather than covering it.
    v.faint_bg_color = if dark { Color32::from_white_alpha(7) } else { Color32::from_black_alpha(9) };
    v.code_bg_color = p.raised;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(RADIUS);
    v.menu_corner_radius = CornerRadius::same(10);
    v.hyperlink_color = p.link;
    v.selection.bg_fill = p.accent.gamma_multiply(if dark { 0.28 } else { 0.22 });
    v.selection.stroke = Stroke::new(1.0, p.accent_strong);

    let r = CornerRadius::same(8);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = r;
    w.inactive.bg_fill = p.raised;
    w.inactive.weak_bg_fill = p.raised;
    // Light mode needs outlines, or inputs and buttons vanish on white cards.
    w.inactive.bg_stroke = if dark { Stroke::NONE } else { Stroke::new(1.0, p.border) };
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = r;
    w.hovered.bg_fill = p.hover;
    w.hovered.weak_bg_fill = p.hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.accent.gamma_multiply(0.6));
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.hovered.corner_radius = r;
    w.active.bg_fill = p.accent.gamma_multiply(0.35);
    w.active.weak_bg_fill = p.accent.gamma_multiply(0.35);
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(2.0, p.text);
    w.active.corner_radius = r;
    w.open = w.hovered;
    v
}

/// A system font for one role, per platform: file names in Windows' Fonts folder, macOS paths
/// with the face in the collection, and fontconfig patterns elsewhere, each tried in order.
#[allow(dead_code)] // each platform reads only its own list
struct SystemFont {
    windows: &'static [&'static str],
    macos: &'static [(&'static str, u32)],
    fontconfig: &'static [&'static str],
}

/// Where a font lives, and its face in a collection.
#[cfg(windows)]
fn font_file(font: &SystemFont) -> Option<(std::path::PathBuf, u32)> {
    let dir = std::env::var_os("WINDIR").map_or_else(|| std::path::PathBuf::from(r"C:\Windows"), std::path::PathBuf::from).join("Fonts");
    font.windows.iter().map(|f| dir.join(f)).find(|p| p.is_file()).map(|p| (p, 0))
}
#[cfg(target_os = "macos")]
fn font_file(font: &SystemFont) -> Option<(std::path::PathBuf, u32)> {
    font.macos.iter().map(|&(p, i)| (std::path::PathBuf::from(p), i)).find(|(p, i)| p.is_file() && is_static(p, *i))
}
#[cfg(all(unix, not(target_os = "macos")))]
fn font_file(font: &SystemFont) -> Option<(std::path::PathBuf, u32)> {
    font.fontconfig.iter().find_map(|pattern| fontconfig_file(pattern))
}

/// What fontconfig picks for `pattern`.
#[cfg(all(unix, not(target_os = "macos")))]
fn fontconfig_file(pattern: &str) -> Option<(std::path::PathBuf, u32)> {
    let fc = |args: &[&str]| -> Option<String> {
        let out = std::process::Command::new(args[0]).args(&args[1..]).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    };
    const FORMAT: &str = "--format=%{family[0]}\t%{style[0]}\t%{index}\t%{lang}\t%{file}\n";
    struct Face<'a> {
        family: &'a str,
        style: &'a str,
        langs: &'a str,
        file: std::path::PathBuf,
        index: u32,
    }
    impl Face<'_> {
        /// egui draws nothing for a variable font's glyphs, so only static faces count.
        /// fontconfig's own flag can't tell: it lists a variable font's default and named
        /// instances as not variable (the latter with the instance in the index's high bits).
        fn usable(&self) -> bool {
            self.index < 0x1_0000 && is_static(&self.file, self.index)
        }
    }
    fn faces(text: &str) -> Vec<Face<'_>> {
        text.lines()
            .filter_map(|l| {
                let mut f = l.splitn(5, '\t');
                let (family, style, index, langs, file) = (f.next()?, f.next()?, f.next()?, f.next()?, f.next()?);
                let index: u32 = index.parse().ok()?;
                let file = std::path::PathBuf::from(file);
                Some(Face { family, style, langs, file, index })
            })
            .collect()
    }
    // fc-list matches exactly, where fc-match substitutes its closest font (NixOS's emoji rules
    // turn "Noto Emoji" into Noto Color Emoji, which egui can't draw). Prefer the upright weight,
    // and a proportional family over a Mono one.
    let list = |pat: &str| -> Option<(std::path::PathBuf, u32)> {
        let text = fc(&["fc-list", pat, FORMAT])?;
        let all = faces(&text);
        let upright = |f: &&Face| matches!(f.style, "Regular" | "Book" | "Normal" | "Roman") && f.usable();
        let face = all.iter().filter(upright).find(|f| !f.family.contains("Mono")).or_else(|| all.iter().find(upright)).or_else(|| all.iter().find(|f| f.usable()))?;
        Some((face.file.clone(), face.index))
    };
    if !pattern.starts_with("sans-serif") {
        return list(pattern);
    }
    let lang = pattern.split_once(":lang=").map(|(_, l)| l);
    // The desktop's own sans-serif, ranked through the user's aliases by `fc-match -s`: the
    // first static face, and with :lang= the first that covers it (a variable Noto Sans CJK JP
    // ranks first, but its static copy comes before Noto Sans CJK SC's Chinese shapes).
    let text = fc(&["fc-match", "-s", pattern, FORMAT])?;
    let face = faces(&text).into_iter().find(|f| lang.is_none_or(|l| f.langs.split('|').any(|x| x == l)) && f.usable())?;
    Some((face.file, face.index))
}

/// Whether face `index` of a font file has no `fvar` table, read from its sfnt table directory
/// (behind the `ttcf` header in a collection).
#[cfg(not(windows))]
fn is_static(path: &std::path::Path, index: u32) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let read = || -> std::io::Result<bool> {
        let mut file = std::fs::File::open(path)?;
        let mut word = [0u8; 4];
        let at = |file: &mut std::fs::File, pos: u64, buf: &mut [u8]| -> std::io::Result<()> {
            file.seek(SeekFrom::Start(pos))?;
            file.read_exact(buf)
        };
        at(&mut file, 0, &mut word)?;
        let mut offset = 0u64;
        if &word == b"ttcf" {
            at(&mut file, 12 + 4 * u64::from(index), &mut word)?;
            offset = u64::from(u32::from_be_bytes(word));
        }
        let mut count = [0u8; 2];
        at(&mut file, offset + 4, &mut count)?;
        let mut dir = vec![0u8; 16 * usize::from(u16::from_be_bytes(count))];
        at(&mut file, offset + 12, &mut dir)?;
        Ok(!dir.as_chunks::<16>().0.iter().any(|record| &record[..4] == b"fvar"))
    };
    read().unwrap_or(false)
}

/// Static subsets of Noto Emoji, DejaVu Sans, DejaVu Sans Mono, Noto Sans Symbols, Noto Sans
/// Math, Noto Sans Symbols 2 and Noto Sans CJK JP (its full-width forms, ＋ among them) with every symbol the app and its data draw (`tools/subset_fonts.py`; licenses beside
/// them). Windows has these in Segoe UI Symbol; elsewhere no font is everywhere, and egui can't
/// draw a variable font (which Noto Emoji is).
const BUNDLED: [(&str, &[u8]); 7] = [
    ("Bundled emoji", include_bytes!("../assets/fonts/NotoEmoji-umaLord.ttf")),
    ("Bundled DejaVu", include_bytes!("../assets/fonts/DejaVuSans-umaLord.ttf")),
    ("Bundled DejaVu Mono", include_bytes!("../assets/fonts/DejaVuSansMono-umaLord.ttf")),
    ("Bundled symbols", include_bytes!("../assets/fonts/NotoSansSymbols-umaLord.ttf")),
    ("Bundled math", include_bytes!("../assets/fonts/NotoSansMath-umaLord.ttf")),
    ("Bundled symbols 2", include_bytes!("../assets/fonts/NotoSansSymbols2-umaLord.ttf")),
    ("Bundled full-width", include_bytes!("../assets/fonts/NotoSansCJKjp-umaLord.ttf")),
];

const REGULAR: SystemFont = SystemFont {
    windows: &["segoeui.ttf"],
    macos: &[("/System/Library/Fonts/Helvetica.ttc", 0), ("/System/Library/Fonts/Supplemental/Arial.ttf", 0)],
    fontconfig: &["sans-serif:weight=regular"],
};
const SEMIBOLD_FONT: SystemFont = SystemFont {
    windows: &["seguisb.ttf"],
    macos: &[("/System/Library/Fonts/Helvetica.ttc", 1), ("/System/Library/Fonts/Supplemental/Arial Bold.ttf", 0)],
    fontconfig: &["sans-serif:weight=semibold"],
};
const SYMBOL: SystemFont = SystemFont { windows: &["seguisym.ttf"], macos: &[("/System/Library/Fonts/Apple Symbols.ttf", 0)], fontconfig: &[] };
const JAPANESE: SystemFont = SystemFont {
    windows: &["YuGothR.ttc", "meiryo.ttc", "msgothic.ttc"],
    macos: &[("/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", 0), ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0)],
    // fontconfig ranks Unifont (a last-resort bitmap font) above a static Noto CJK, so ask for
    // the Japanese Noto by name first.
    fontconfig: &["Noto Sans CJK JP", "sans-serif:lang=ja"],
};

/// Segoe UI (Windows' UI font) for text, Segoe UI Semibold for headings, then egui's own fonts,
/// Segoe UI Symbol for ☆ ♪ ≠ ∞ ♡, a Japanese font for event text, and the bundled symbol fonts
/// for anything those lack. macOS uses Helvetica, Apple Symbols and Hiragino; other systems ask
/// fontconfig for the desktop's sans-serif and a Japanese sans. Falls back to egui's defaults
/// where a font is missing.
fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let mut loaded: Vec<(std::path::PathBuf, u32, String)> = Vec::new();
    let mut load = |name: &str, font: &SystemFont| -> Option<String> {
        let (path, index) = font_file(font)?;
        // fontconfig may answer with a font already loaded (no semibold gives the regular one):
        // reuse it, so each chain still gets its glyphs.
        if let Some((.., known)) = loaded.iter().find(|(p, i, _)| *p == path && *i == index) {
            return Some(known.clone());
        }
        let bytes = std::fs::read(&path).ok()?;
        loaded.push((path, index, name.to_string()));
        let mut data = FontData::from_owned(bytes);
        data.index = index;
        defs.font_data.insert(name.to_string(), Arc::new(data));
        Some(name.to_string())
    };
    let regular = load("Regular", &REGULAR);
    let semibold = load("Semibold", &SEMIBOLD_FONT);
    let mut fallbacks: Vec<String> = [load("Symbol", &SYMBOL), load("Japanese", &JAPANESE)].into_iter().flatten().collect();
    for (name, bytes) in BUNDLED {
        defs.font_data.insert(name.to_string(), Arc::new(FontData::from_static(bytes)));
        fallbacks.push(name.to_string());
    }

    let base_prop = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let base_mono = defs.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    let chain = |first: Vec<String>, base: &[String]| -> Vec<String> {
        let mut v = first;
        v.extend(base.iter().cloned());
        v.extend(fallbacks.iter().cloned());
        v
    };
    defs.families.insert(FontFamily::Proportional, chain(regular.clone().into_iter().collect(), &base_prop));
    defs.families.insert(FontFamily::Monospace, chain(Vec::new(), &base_mono));
    defs.families.insert(FontFamily::Name(SEMIBOLD.into()), chain(semibold.or(regular).into_iter().collect(), &base_prop));
    ctx.set_fonts(defs);
}

/// Spacing for the current density.
fn apply_spacing(s: &mut egui::Style) {
    let d = density();
    s.spacing.item_spacing = egui::vec2(8.0 * d, 6.0 * d);
    s.spacing.button_padding = egui::vec2(10.0 * d, 5.0 * d);
    s.spacing.interact_size.y = 24.0;
    s.spacing.window_margin = Margin::same((12.0 * d) as i8);
    s.spacing.menu_margin = Margin::same((8.0 * d) as i8);
}

/// Fonts, colours and spacing, starting in `mode` (System follows the OS).
pub fn install(ctx: &egui::Context, mode: Mode) {
    install_fonts(ctx);
    for theme in [Theme::Dark, Theme::Light] {
        ctx.set_visuals_of(theme, visuals(theme == Theme::Dark));
        ctx.style_mut_of(theme, |s| {
            s.text_styles = [
                (TextStyle::Small, FontId::proportional(size::XS)),
                (TextStyle::Body, FontId::proportional(size::BODY)),
                (TextStyle::Button, FontId::proportional(size::BODY)),
                (TextStyle::Monospace, FontId::monospace(size::SM)),
                (TextStyle::Heading, FontId::new(size::XL, FontFamily::Name(SEMIBOLD.into()))),
            ]
            .into();
            apply_spacing(s);
        });
    }
    set_mode(ctx, mode);
}

pub fn set_mode(ctx: &egui::Context, mode: Mode) {
    ctx.set_theme(mode.preference());
}

/// Whether the running app is drawn dark (System resolves to what Windows uses).
pub fn is_dark(ctx: &egui::Context) -> bool {
    ctx.theme() == Theme::Dark
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The macOS font paths are written from Apple's layout, not looked up, so CI checks them.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_fonts_exist_and_are_static() {
        for (role, font) in [("regular", &REGULAR), ("semibold", &SEMIBOLD_FONT), ("symbol", &SYMBOL), ("Japanese", &JAPANESE)] {
            assert!(font_file(font).is_some(), "no static {role} font at {:?}", font.macos);
        }
    }

    /// Status colours, weak text, gold and links read as text (WCAG AA, 4.5:1) on cards and on
    /// the page, in both modes and every theme.
    #[test]
    fn text_colours_pass_contrast() {
        for look in Look::ALL {
            for dark in [true, false] {
                let p = palette_for(look, dark);
                for (name, c) in [("good", p.good), ("warn", p.warn), ("bad", p.bad), ("weak", p.weak), ("gold", p.gold), ("link", p.link), ("pink", p.pink)] {
                    for (on, bg) in [("surface", p.surface), ("page", p.bg)] {
                        let r = contrast(c, bg);
                        assert!(r >= 4.5, "{name} on {on}, {look:?} {}: {r:.2}", if dark { "dark" } else { "light" });
                    }
                }
                // Text on the accent (main buttons).
                let r = contrast(p.on_accent, p.accent);
                assert!(r >= 4.5, "text on the accent, {look:?} {}: {r:.2}", if dark { "dark" } else { "light" });
            }
        }
    }

    #[test]
    fn contrast_matches_wcag() {
        assert!((contrast(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.01);
        assert!((contrast(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 0.001);
    }

    #[test]
    fn settings_keys_round_trip() {
        for m in Mode::ALL {
            assert_eq!(Mode::from_key(m.key()), m);
        }
        assert_eq!(Mode::from_key("nonsense"), Mode::Dark, "old settings default to dark");
        for c in Chibi::ALL {
            assert_eq!(Chibi::from_key(c.key()), Some(c));
        }
    }
}
