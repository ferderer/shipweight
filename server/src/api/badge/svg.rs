/// Verdana 11px character width table (same font as shields.io / badgen).
fn char_width(c: char) -> f64 {
    match c {
        ' ' => 3.3,
        'f' | 'i' | 'j' | 'l' | 't' => 4.0,
        'r' => 4.4,
        '!' | '(' | ')' | '[' | ']' | '{' | '}' | '|' => 3.6,
        '1' => 5.5,
        '.' | ',' | ':' | ';' => 3.3,
        '-' => 4.4,
        'a' | 'c' | 'e' | 'g' | 'o' | 's' | 'z' => 6.2,
        'b' | 'd' | 'h' | 'k' | 'n' | 'p' | 'q' | 'u' | 'v' | 'x' | 'y' => 6.5,
        'w' => 9.1,
        'm' => 9.7,
        '0' | '2'..='9' => 6.5,
        'A'..='N' | 'P'..='V' | 'X'..='Z' => 7.5,
        'M' => 8.6,
        'W' => 10.4,
        _ => 7.0,
    }
}

pub fn text_width(s: &str) -> f64 {
    s.chars().map(char_width).sum()
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1_000 {
        return format!("{} B", bytes);
    }
    let kb = bytes as f64 / 1_000.0;
    if kb < 1_000.0 {
        return if kb < 10.0 {
            format!("{:.2} kB", kb)
        } else if kb < 100.0 {
            format!("{:.1} kB", kb)
        } else {
            format!("{:.0} kB", kb)
        };
    }
    let mb = kb / 1_000.0;
    if mb < 100.0 { format!("{:.1} MB", mb) } else { format!("{:.0} MB", mb) }
}

pub fn format_downloads(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.0}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

// ---------------------------------------------------------------------------
// Skins
// ---------------------------------------------------------------------------

pub struct Skin {
    pub label_bg: &'static str,
    pub label_fg: &'static str,
    pub value_fg: &'static str,
    pub font: &'static str,
    pub font_size: u32,
    pub height: u32,
    pub radius: u32,
    pub shadow: bool,
}

pub fn skin(name: &str) -> Skin {
    match name {
        "flat-square" => Skin {
            label_bg: "#555",
            label_fg: "#fff",
            value_fg: "#fff",
            font: "Verdana,Geneva,DejaVu Sans,sans-serif",
            font_size: 11,
            height: 20,
            radius: 0,
            shadow: false,
        },
        "neon" => Skin {
            label_bg: "#0d0d0d",
            label_fg: "#aaa",
            value_fg: "#fff",
            font: "Verdana,Geneva,DejaVu Sans,sans-serif",
            font_size: 11,
            height: 20,
            radius: 4,
            shadow: true,
        },
        "terminal" => Skin {
            label_bg: "#0a0a0a",
            label_fg: "#33ff33",
            value_fg: "#33ff33",
            font: "'Courier New',Courier,monospace",
            font_size: 11,
            height: 20,
            radius: 0,
            shadow: false,
        },
        "minimal" => Skin {
            label_bg: "#eee",
            label_fg: "#555",
            value_fg: "#333",
            font: "Verdana,Geneva,DejaVu Sans,sans-serif",
            font_size: 10,
            height: 18,
            radius: 2,
            shadow: false,
        },
        "retro" => Skin {
            label_bg: "#888",
            label_fg: "#fff",
            value_fg: "#fff",
            font: "Verdana,Geneva,DejaVu Sans,sans-serif",
            font_size: 11,
            height: 20,
            radius: 2,
            shadow: false,
        },
        // default: flat
        _ => Skin {
            label_bg: "#555",
            label_fg: "#fff",
            value_fg: "#fff",
            font: "Verdana,Geneva,DejaVu Sans,sans-serif",
            font_size: 11,
            height: 20,
            radius: 3,
            shadow: true,
        },
    }
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

pub struct BadgeSegment<'a> {
    pub label: &'a str,
    pub value: &'a str,
    pub color: &'a str,
}

/// Render one or more segments as a single SVG badge.
pub fn render(segments: &[BadgeSegment<'_>], skin_name: &str) -> String {
    let s = skin(skin_name);
    let pad = 10.0_f64;
    let text_y = (s.height as f64 * 0.7).round() as u32;
    let shadow_y = text_y + 1;

    // Calculate widths per segment (label + value each get their own box)
    struct Seg {
        label_w: f64,
        value_w: f64,
    }
    let dims: Vec<Seg> = segments
        .iter()
        .map(|seg| Seg {
            label_w: (text_width(seg.label) + pad * 2.0).ceil(),
            value_w: (text_width(seg.value) + pad * 2.0).ceil(),
        })
        .collect();

    let total_w: f64 = dims.iter().map(|d| d.label_w + d.value_w).sum();
    let h = s.height;
    let r = s.radius;

    let mut rects = String::new();
    let mut texts = String::new();
    let mut x = 0.0_f64;

    for (i, (seg, dim)) in segments.iter().zip(dims.iter()).enumerate() {
        let lw = dim.label_w;
        let vw = dim.value_w;
        let lx = (x + lw / 2.0).round() as u32;
        let vx = (x + lw + vw / 2.0).round() as u32;

        // Label rect
        rects.push_str(&format!(
            r#"<rect x="{x}" width="{lw}" height="{h}" fill="{bg}"/>"#,
            x = x.round() as u32,
            lw = lw as u32,
            h = h,
            bg = s.label_bg,
        ));
        // Value rect
        rects.push_str(&format!(
            r#"<rect x="{vx_start}" width="{vw}" height="{h}" fill="{color}"/>"#,
            vx_start = (x + lw).round() as u32,
            vw = vw as u32,
            h = h,
            color = seg.color,
        ));

        // Separator line between segments (skip first)
        if i > 0 {
            rects.push_str(&format!(
                r#"<rect x="{x}" width="1" height="{h}" fill="#ffffff22"/>"#,
                x = x.round() as u32,
                h = h,
            ));
        }

        if s.shadow {
            texts.push_str(&format!(
                r#"<text x="{lx}" y="{sy}" fill="#010101" fill-opacity=".3">{label}</text>"#,
                lx = lx, sy = shadow_y, label = xml_escape(seg.label),
            ));
            texts.push_str(&format!(
                r#"<text x="{vx}" y="{sy}" fill="#010101" fill-opacity=".3">{value}</text>"#,
                vx = vx, sy = shadow_y, value = xml_escape(seg.value),
            ));
        }
        texts.push_str(&format!(
            r#"<text x="{lx}" y="{ty}" fill="{lfg}">{label}</text>"#,
            lx = lx, ty = text_y, lfg = s.label_fg, label = xml_escape(seg.label),
        ));
        texts.push_str(&format!(
            r#"<text x="{vx}" y="{ty}" fill="{vfg}">{value}</text>"#,
            vx = vx, ty = text_y, vfg = s.value_fg, value = xml_escape(seg.value),
        ));

        x += lw + vw;
    }

    // Clip path for rounded corners
    let clip = if r > 0 {
        format!(
            r#"<clipPath id="r"><rect width="{tw}" height="{h}" rx="{r}" fill="#fff"/></clipPath>"#,
            tw = total_w as u32, h = h, r = r,
        )
    } else {
        String::new()
    };
    let clip_attr = if r > 0 { r#" clip-path="url(#r)""# } else { "" };

    // Neon glow filter
    let (filter_def, filter_attr) = if skin_name == "neon" {
        (
            r#"<filter id="glow"><feGaussianBlur stdDeviation="1.5" result="blur"/><feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge></filter>"#,
            r#" filter="url(#glow)""#,
        )
    } else {
        ("", "")
    };

    // Gradient overlay for flat/retro skins
    let gradient = if s.shadow {
        r#"<linearGradient id="s" x2="0" y2="100%"><stop offset="0" stop-color="#bbb" stop-opacity=".1"/><stop offset="1" stop-opacity=".1"/></linearGradient>"#
    } else {
        ""
    };
    let gradient_rect = if s.shadow {
        format!(r#"<rect width="{tw}" height="{h}" fill="url(#s)"/>"#, tw = total_w as u32, h = h)
    } else {
        String::new()
    };

    // Build aria label from all segments
    let aria: String = segments
        .iter()
        .map(|s| format!("{}: {}", s.label, s.value))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{tw}" height="{h}" role="img" aria-label="{aria}"><title>{aria}</title><defs>{gradient}{clip}{filter_def}</defs><g{clip_attr}>{rects}{gradient_rect}</g><g{filter_attr} text-anchor="middle" font-family="{font}" text-rendering="geometricPrecision" font-size="{fs}">{texts}</g></svg>"#,
        tw = total_w as u32,
        h = h,
        aria = xml_escape(&aria),
        gradient = gradient,
        clip = clip,
        filter_def = filter_def,
        clip_attr = clip_attr,
        rects = rects,
        gradient_rect = gradient_rect,
        filter_attr = filter_attr,
        font = s.font,
        fs = s.font_size,
        texts = texts,
    )
}

pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
