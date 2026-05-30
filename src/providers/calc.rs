use crate::engine::Provider;
use crate::model::{Action, Item};

/// Inline calculator. Evaluates basic arithmetic expressions with a small
/// hand-rolled tokenizer + shunting-yard evaluator (no third-party crate).
pub struct CalcProvider;

impl Provider for CalcProvider {
    fn name(&self) -> &'static str {
        "calc"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let expr = query.trim();
        // Require at least one operator so plain numbers/words are ignored.
        if !expr.bytes().any(|b| matches!(b, b'+' | b'-' | b'*' | b'/' | b'%' | b'^')) {
            return;
        }
        if let Some(value) = eval(expr) {
            if value.is_finite() {
                let formatted = format_number(value);
                out.push(Item::new(
                    format!("= {formatted}"),
                    "Press Enter to copy the result",
                    "Calc",
                    // Rank above everything: an explicit math query is unambiguous.
                    10_000,
                    Action::CopyText(formatted),
                ));
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    Num(f64),
    Op(char),
    LParen,
    RParen,
}

fn tokenize(input: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut prev_value_or_rparen = false;
    while i < bytes.len() {
        let c = bytes[i] as char;
        match c {
            ' ' | '\t' => {
                i += 1;
            }
            '0'..='9' | '.' => {
                let start = i;
                while i < bytes.len()
                    && (bytes[i].is_ascii_digit() || bytes[i] == b'.')
                {
                    i += 1;
                }
                let num: f64 = input[start..i].parse().ok()?;
                tokens.push(Token::Num(num));
                prev_value_or_rparen = true;
            }
            '+' | '-' | '*' | '/' | '%' | '^' => {
                // Distinguish unary minus/plus from binary.
                if (c == '-' || c == '+') && !prev_value_or_rparen {
                    // Unary: represent as (0 <op> ...).
                    tokens.push(Token::Num(0.0));
                }
                tokens.push(Token::Op(c));
                prev_value_or_rparen = false;
                i += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                prev_value_or_rparen = false;
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                prev_value_or_rparen = true;
                i += 1;
            }
            _ => return None,
        }
    }
    Some(tokens)
}

fn precedence(op: char) -> u8 {
    match op {
        '+' | '-' => 1,
        '*' | '/' | '%' => 2,
        '^' => 3,
        _ => 0,
    }
}

fn right_associative(op: char) -> bool {
    op == '^'
}

fn eval(input: &str) -> Option<f64> {
    let tokens = tokenize(input)?;
    if tokens.is_empty() {
        return None;
    }
    // Shunting-yard into RPN.
    let mut output: Vec<Token> = Vec::new();
    let mut ops: Vec<Token> = Vec::new();
    for token in tokens {
        match token {
            Token::Num(_) => output.push(token),
            Token::Op(o1) => {
                while let Some(Token::Op(o2)) = ops.last().copied() {
                    let higher = precedence(o2) > precedence(o1)
                        || (precedence(o2) == precedence(o1) && !right_associative(o1));
                    if higher {
                        output.push(ops.pop().unwrap());
                    } else {
                        break;
                    }
                }
                ops.push(token);
            }
            Token::LParen => ops.push(token),
            Token::RParen => {
                let mut found = false;
                while let Some(top) = ops.pop() {
                    if top == Token::LParen {
                        found = true;
                        break;
                    }
                    output.push(top);
                }
                if !found {
                    return None;
                }
            }
        }
    }
    while let Some(top) = ops.pop() {
        if top == Token::LParen || top == Token::RParen {
            return None;
        }
        output.push(top);
    }

    // Evaluate RPN.
    let mut stack: Vec<f64> = Vec::new();
    for token in output {
        match token {
            Token::Num(n) => stack.push(n),
            Token::Op(op) => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                let r = match op {
                    '+' => a + b,
                    '-' => a - b,
                    '*' => a * b,
                    '/' => a / b,
                    '%' => a % b,
                    '^' => a.powf(b),
                    _ => return None,
                };
                stack.push(r);
            }
            _ => return None,
        }
    }
    if stack.len() == 1 {
        Some(stack[0])
    } else {
        None
    }
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        // Trim trailing zeros from a reasonable precision.
        let s = format!("{value:.10}");
        let s = s.trim_end_matches('0').trim_end_matches('.');
        s.to_string()
    }
}
