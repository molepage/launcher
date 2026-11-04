//! Inline calculator: + - * / (also x × ÷), ^, postfix %, brackets.

pub fn eval(input: &str) -> Option<f64> {
    let s = input.trim();
    // Only answer things that look like maths: a digit and an operator.
    if !s.chars().any(|c| c.is_ascii_digit()) || !s.chars().any(|c| "+-*/x×÷^%()".contains(c)) {
        return None;
    }
    let toks = lex(s)?;
    if toks.len() < 2 {
        return None;
    }
    let mut p = Parser { toks, i: 0 };
    let v = p.expr()?;
    (p.i == p.toks.len() && v.is_finite()).then_some(v)
}

pub fn format(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let s = format!("{:.10}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum T {
    Num(f64),
    Op(char),
}

fn lex(s: &str) -> Option<Vec<T>> {
    let mut out = Vec::new();
    let cs: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        match c {
            ' ' | ',' | '_' => i += 1,
            '0'..='9' | '.' => {
                let st = i;
                while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.' || cs[i] == ',' || cs[i] == '_') {
                    i += 1;
                }
                let n: String = cs[st..i].iter().filter(|c| **c != ',' && **c != '_').collect();
                out.push(T::Num(n.parse().ok()?));
            }
            'x' | 'X' | '×' | '*' => {
                out.push(T::Op('*'));
                i += 1
            }
            '÷' | '/' => {
                out.push(T::Op('/'));
                i += 1
            }
            '+' | '-' | '^' | '%' | '(' | ')' => {
                out.push(T::Op(c));
                i += 1
            }
            _ => return None,
        }
    }
    Some(out)
}

struct Parser {
    toks: Vec<T>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<T> {
        self.toks.get(self.i).copied()
    }
    fn eat(&mut self, op: char) -> bool {
        if self.peek() == Some(T::Op(op)) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        loop {
            if self.eat('+') {
                v += self.term()?
            } else if self.eat('-') {
                v -= self.term()?
            } else {
                return Some(v);
            }
        }
    }
    fn term(&mut self) -> Option<f64> {
        let mut v = self.power()?;
        loop {
            if self.eat('*') {
                v *= self.power()?
            } else if self.eat('/') {
                v /= self.power()?
            } else {
                return Some(v);
            }
        }
    }
    fn power(&mut self) -> Option<f64> {
        let base = self.unary()?;
        if self.eat('^') { Some(base.powf(self.power()?)) } else { Some(base) }
    }
    fn unary(&mut self) -> Option<f64> {
        if self.eat('-') {
            return Some(-self.unary()?);
        }
        if self.eat('+') {
            return self.unary();
        }
        self.postfix()
    }
    fn postfix(&mut self) -> Option<f64> {
        let mut v = match self.peek()? {
            T::Num(n) => {
                self.i += 1;
                n
            }
            T::Op('(') => {
                self.i += 1;
                let v = self.expr()?;
                if !self.eat(')') {
                    return None;
                }
                v
            }
            _ => return None,
        };
        while self.eat('%') {
            v /= 100.0;
        }
        Some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic() {
        assert_eq!(eval("2+2"), Some(4.0));
        assert_eq!(eval("2 + 3 * 4"), Some(14.0));
        assert_eq!(eval("(2+3)*4"), Some(20.0));
        assert_eq!(eval("2^3^2"), Some(512.0));
        assert_eq!(eval("-3 + 1"), Some(-2.0));
        assert_eq!(eval("200 * 15%"), Some(30.0));
        assert_eq!(eval("10 x 3"), Some(30.0));
        assert_eq!(eval("1,000 / 4"), Some(250.0));
        assert_eq!(eval("9 ÷ 3"), Some(3.0));
    }

    #[test]
    fn rejects_non_maths() {
        assert_eq!(eval("safari"), None);
        assert_eq!(eval("42"), None);
        assert_eq!(eval("2+"), None);
        assert_eq!(eval("1/0"), None);
        assert_eq!(eval("excel 2"), None);
    }

    #[test]
    fn formatting() {
        assert_eq!(format(4.0), "4");
        assert_eq!(format(0.1 + 0.2), "0.3");
        assert_eq!(format(1.0 / 3.0), "0.3333333333");
    }
}
