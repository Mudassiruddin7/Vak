use super::script::major_class;

const TERMINATORS: [char; 5] = ['.', '?', '!', '।', '॥'];

/// Split on sentence terminators (including the danda) that are followed by whitespace, and on newlines.
pub fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\n' {
            push_trimmed(&mut out, &mut current);
            continue;
        }
        current.push(c);
        if TERMINATORS.contains(&c) {
            match chars.peek() {
                Some(next) if next.is_whitespace() => push_trimmed(&mut out, &mut current),
                None => push_trimmed(&mut out, &mut current),
                _ => {}
            }
        }
    }
    push_trimmed(&mut out, &mut current);
    out
}

fn push_trimmed(out: &mut Vec<String>, current: &mut String) {
    let t = current.trim();
    if !t.is_empty() {
        out.push(t.to_string());
    }
    current.clear();
}

/// Words that contain at least one letter, digit or combining mark. Bare dashes and stray symbols don't count.
pub fn word_count(sentence: &str) -> usize {
    sentence.split_whitespace().filter(|w| w.chars().any(|c| matches!(major_class(c), 'L' | 'M' | 'N'))).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_terminators_danda_and_newlines() {
        assert_eq!(split_sentences("ఇది ఒకటి. ఇది రెండు? సరే!"), vec!["ఇది ఒకటి.", "ఇది రెండు?", "సరే!"]);
        assert_eq!(split_sentences("यह पहला है। यह दूसरा है॥ तीसरा"), vec!["यह पहला है।", "यह दूसरा है॥", "तीसरा"]);
        assert_eq!(split_sentences("one\ntwo.three four. five"), vec!["one", "two.three four.", "five"]);
        assert_eq!(split_sentences("3.5 కిలోలు"), vec!["3.5 కిలోలు"]);
        assert!(split_sentences("  \n ").is_empty());
    }

    #[test]
    fn counts_words_not_dashes() {
        assert_eq!(word_count("ఇది — ఒక వాక్యం ."), 3);
        assert_eq!(word_count(&vec!["పదం"; 15].join(" ")), 15);
    }
}
