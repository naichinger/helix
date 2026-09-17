//! Quiet neutral surfaces and restrained accents, inspired by Zeron's desktop UI.
use gpui::{rgb, Hsla};
use helix_view::graphics::Color;

pub struct Palette {
    pub shell: Hsla,
    pub panel: Hsla,
    pub raised: Hsla,
    pub hover: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
}

impl Palette {
    pub fn for_background(background: Color) -> Self {
        let light = match background {
            Color::Rgb(r, g, b) => r as u32 * 299 + g as u32 * 587 + b as u32 * 114 > 150_000,
            Color::White | Color::LightGray => true,
            _ => false,
        };
        let [shell, panel, raised, hover, border, text, muted, accent] = if light {
            [
                0xf0f0f2, 0xfafafa, 0xffffff, 0xe8e8ec, 0xd6d6dc, 0x232328, 0x71717b, 0x6554c0,
            ]
        } else {
            [
                0x171718, 0x101011, 0x252527, 0x29292d, 0x303034, 0xe4e4e7, 0x929299, 0xa5a0f7,
            ]
        };
        Self {
            shell: rgb(shell).into(),
            panel: rgb(panel).into(),
            raised: rgb(raised).into(),
            hover: rgb(hover).into(),
            border: rgb(border).into(),
            text: rgb(text).into(),
            muted: rgb(muted).into(),
            accent: rgb(accent).into(),
        }
    }
}
