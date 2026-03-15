/// Validate an npm package name. Returns `Some(reason)` if invalid.
pub fn invalid_npm_name(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        return Some("empty name");
    }
    if name.len() > 214 {
        return Some("name too long");
    }
    if name.starts_with('.') || name.starts_with('_') {
        return Some("invalid prefix");
    }

    let check = if let Some(rest) = name.strip_prefix('@') {
        match rest.split_once('/') {
            Some((scope, pkg)) if !scope.is_empty() && !pkg.is_empty() => {
                if !is_valid_npm_chars(scope) {
                    return Some("invalid characters in scope");
                }
                pkg
            }
            _ => return Some("invalid scoped name"),
        }
    } else {
        name
    };

    if !is_valid_npm_chars(check) {
        return Some("invalid characters");
    }

    if is_spam(name) {
        return Some("spam pattern");
    }

    None
}

fn is_valid_npm_chars(s: &str) -> bool {
    s.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.' || c == '_')
}

fn is_spam(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();

    let hyphens = lower.chars().filter(|&c| c == '-').count();
    if hyphens >= 5 && lower.len() > 40 {
        return true;
    }

    const SPAM_WORDS: &[&str] = &[
        "discount", "coupon", "promo-code", "buy-", "cheap-", "free-download",
        "crack-", "keygen", "serial-key", "license-key", "activation-code",
        "watch-online", "full-movie", "download-free", "hack-tool",
        "weight-loss", "diet-pill", "forex-", "casino-", "betting-",
        "crypto-earn", "airdrop-", "nft-mint",
    ];
    SPAM_WORDS.iter().any(|w| lower.contains(w))
}
