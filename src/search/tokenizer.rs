//! Two explicit code analyzers: exact identifiers and readable snake/camel/path parts.
use tantivy::tokenizer::{Token, TokenStream, Tokenizer};

#[derive(Clone, Copy, Debug)]
pub struct CodeTokenizer {
    pub split_parts: bool,
}

pub struct CodeTokenStream<'a> {
    text: &'a str,
    cursor: usize,
    split_parts: bool,
    pending: std::vec::IntoIter<Token>,
    position: usize,
    current: Token,
}

impl Tokenizer for CodeTokenizer {
    type TokenStream<'a> = CodeTokenStream<'a>;
    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        CodeTokenStream {
            text,
            cursor: 0,
            split_parts: self.split_parts,
            pending: Vec::new().into_iter(),
            position: 0,
            current: Token::default(),
        }
    }
}

fn push(text: &str, from: usize, to: usize, tokens: &mut Vec<Token>) {
    if from < to && text[from..to].chars().count() <= 255 {
        tokens.push(Token {
            offset_from: from,
            offset_to: to,
            position: tokens.len(),
            text: text[from..to].to_lowercase(),
            position_length: 1,
        });
    }
}

fn parts(text: &str, from: usize, to: usize, tokens: &mut Vec<Token>) {
    let chars = text[from..to].char_indices().collect::<Vec<_>>();
    let mut begin = from;
    for (index, (relative, character)) in chars.iter().copied().enumerate() {
        let at = from + relative;
        if character == '_' {
            push(text, begin, at, tokens);
            begin = at + 1;
            continue;
        }
        let previous = index.checked_sub(1).map(|index| chars[index].1);
        let next = chars.get(index + 1).map(|(_, character)| *character);
        let boundary = previous.is_some_and(|previous| {
            (previous.is_lowercase() && character.is_uppercase())
                || (previous.is_uppercase()
                    && character.is_uppercase()
                    && next.is_some_and(char::is_lowercase))
                || (previous.is_numeric() != character.is_numeric() && previous != '_')
        });
        if boundary {
            push(text, begin, at, tokens);
            begin = at;
        }
    }
    push(text, begin, to, tokens);
}

impl TokenStream for CodeTokenStream<'_> {
    fn advance(&mut self) -> bool {
        loop {
            if let Some(mut token) = self.pending.next() {
                token.position = self.position;
                self.position += 1;
                self.current = token;
                return true;
            }
            let Some((relative, _)) = self.text[self.cursor..]
                .char_indices()
                .find(|(_, character)| character.is_alphanumeric() || *character == '_')
            else {
                return false;
            };
            let from = self.cursor + relative;
            let to = self.text[from..]
                .char_indices()
                .find(|(_, character)| !(character.is_alphanumeric() || *character == '_'))
                .map_or(self.text.len(), |(relative, _)| from + relative);
            self.cursor = to;
            // Deliberately bound identifiers, including pathological multi-MiB generated words.
            // Natural language/code tokens longer than255 Unicode scalars are not indexed.
            if self.text[from..to].chars().count() > 255 {
                continue;
            }
            let mut tokens = Vec::new();
            if self.split_parts {
                parts(self.text, from, to, &mut tokens);
            } else {
                push(self.text, from, to, &mut tokens);
            }
            self.pending = tokens.into_iter();
        }
    }
    fn token(&self) -> &Token {
        &self.current
    }
    fn token_mut(&mut self) -> &mut Token {
        &mut self.current
    }
}

pub fn analyze(text: &str, split_parts: bool) -> Vec<Token> {
    let mut tokenizer = CodeTokenizer { split_parts };
    let mut stream = tokenizer.token_stream(text);
    let mut tokens = Vec::new();
    while stream.advance() {
        tokens.push(stream.token().clone());
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_relevance_code_analyzers_preserve_identifiers_parts_phrases_and_unicode_offsets() {
        let text = "src/HTTPServer2/auth_boundary.rs 你好 AuthBoundary safe state";
        let exact = analyze(text, false);
        assert!(exact.iter().any(|token| token.text == "auth_boundary"));
        assert!(exact.iter().any(|token| token.text == "httpserver2"));
        assert!(exact.iter().any(|token| token.text == "authboundary"));
        let parts = analyze(text, true);
        assert_eq!(
            parts
                .iter()
                .map(|token| token.text.as_str())
                .collect::<Vec<_>>(),
            [
                "src", "http", "server", "2", "auth", "boundary", "rs", "你好", "auth", "boundary",
                "safe", "state"
            ]
        );
        for (position, token) in parts.iter().enumerate() {
            assert_eq!(token.position, position);
            assert!(
                text.is_char_boundary(token.offset_from) && text.is_char_boundary(token.offset_to)
            );
            assert_eq!(
                text[token.offset_from..token.offset_to].to_lowercase(),
                token.text
            );
        }
        assert!(analyze(&"x".repeat(256), false).is_empty());
        let mut tokenizer = CodeTokenizer { split_parts: true };
        let huge = format!("needle {}", "abc ".repeat(500_000));
        let mut stream = tokenizer.token_stream(&huge);
        assert!(stream.advance());
        assert_eq!(stream.token().text, "needle", "Stream creates only the current small identifier rather than allocating all source tokens");
    }
}
