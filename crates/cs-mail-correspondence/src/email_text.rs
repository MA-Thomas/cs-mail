//! Local text interpretation of already sanitized HTML. Uses the same HTML5
//! tokenizer as the sanitizer; no URL resolution, resource loading or execution.
use html5ever::{
    buffer_queue::BufferQueue,
    tokenizer::{
        TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts, states::RawKind,
    },
};
use std::cell::RefCell;
#[derive(Default)]
struct TextState {
    text: String,
    style: bool,
    pre: usize,
}
#[derive(Default)]
struct TextSink(RefCell<TextState>);
impl TokenSink for TextSink {
    type Handle = ();
    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        let mut state = self.0.borrow_mut();
        match token {
            Token::TagToken(tag) => {
                let name = tag.name.as_ref();
                if name == "style" {
                    state.style = tag.kind == TagKind::StartTag;
                    return if state.style {
                        TokenSinkResult::RawData(RawKind::Rawtext)
                    } else {
                        TokenSinkResult::Continue
                    };
                }
                if name == "pre" {
                    if tag.kind == TagKind::StartTag {
                        state.pre += 1;
                    } else {
                        state.pre = state.pre.saturating_sub(1);
                    }
                }
                if matches!(
                    name,
                    "address"
                        | "article"
                        | "aside"
                        | "blockquote"
                        | "br"
                        | "dd"
                        | "div"
                        | "dl"
                        | "dt"
                        | "footer"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                        | "header"
                        | "hr"
                        | "li"
                        | "ol"
                        | "p"
                        | "pre"
                        | "section"
                        | "table"
                        | "td"
                        | "th"
                        | "tr"
                        | "ul"
                ) && !state.text.ends_with('\n')
                {
                    state.text.push('\n');
                }
            }
            Token::CharacterTokens(value) if !state.style => {
                if state.pre > 0 {
                    state.text.push_str(&value);
                } else {
                    for c in value.chars() {
                        if c.is_whitespace() {
                            if !state.text.ends_with([' ', '\n']) {
                                state.text.push(' ');
                            }
                        } else {
                            state.text.push(c);
                        }
                    }
                }
            }
            _ => (),
        }
        TokenSinkResult::Continue
    }
}
pub(super) fn extract(html: &str) -> String {
    let input = BufferQueue::default();
    input.push_back(html.into());
    let tokenizer = Tokenizer::new(TextSink::default(), TokenizerOpts::default());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    tokenizer.sink.0.into_inner().text.trim().to_owned()
}
