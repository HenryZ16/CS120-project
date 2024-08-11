/*
symrs is implemented based on the idea of visitor pattern.
Since there's no class inheritance in Rust, we use enum to represent the type of expression:
    leaves: IntNumber, FixedNumber, FloatNumber, Variable;
    unaryOp: neg, sin, cos;
    binaryOp: add, sub, mul, div;
Then, we define some traits, including the built-in trait `Add`, `Sub`, `Mul`, `Div`, `Neg`,
and the custom trait `Visitor` containing `sin`, `cos`, `eval`.
Finally, we implement the built-in traits and `Visitor` trait for each expression type.
*/

use std::fmt;
use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Debug, Clone)]
pub enum Expr {
    // Identifiers
    IntNumber(i32),
    FixedNumber(i32, i32),
    FloatNumber(f64),
    Variable(String),

    // Unary operators
    Neg(Box<Expr>),
    Sin(Box<Expr>),
    Cos(Box<Expr>),

    // Binary operators
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
}

pub trait Visitor {
    fn sin(e: Expr) -> Expr;
    fn cos(e: Expr) -> Expr;
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            // Identifiers
            Expr::IntNumber(n) => write!(f, "{}", n),
            Expr::FixedNumber(n, d) => write!(f, "{}.{}", n, d),
            Expr::FloatNumber(n) => write!(f, "{}", n),
            Expr::Variable(v) => write!(f, "{}", v),

            // Unary operators
            Expr::Neg(e) => match **e {
                Expr::IntNumber(_)
                | Expr::FixedNumber(_, _)
                | Expr::FloatNumber(_)
                | Expr::Variable(_)
                | Expr::Sin(_)
                | Expr::Cos(_) => write!(f, "-{}", e),
                _ => write!(f, "-({})", e),
            },
            Expr::Sin(e) => write!(f, "sin({})", e),
            Expr::Cos(e) => write!(f, "cos({})", e),

            // Binary operators
            Expr::Add(e1, e2) => {
                let s2 = match **e2 {
                    Expr::IntNumber(_)
                    | Expr::FixedNumber(_, _)
                    | Expr::FloatNumber(_)
                    | Expr::Variable(_)
                    | Expr::Sin(_)
                    | Expr::Cos(_)
                    | Expr::Mul(_, _)
                    | Expr::Div(_, _) => format!("{}", e2),
                    _ => format!("({})", e2),
                };
                write!(f, "{} + {}", e1, s2)
            }
            Expr::Sub(e1, e2) => {
                let s2 = match **e2 {
                    Expr::IntNumber(_)
                    | Expr::FixedNumber(_, _)
                    | Expr::FloatNumber(_)
                    | Expr::Variable(_)
                    | Expr::Sin(_)
                    | Expr::Cos(_)
                    | Expr::Mul(_, _)
                    | Expr::Div(_, _) => format!("{}", e2),
                    _ => format!("({})", e2),
                };
                write!(f, "{} - {}", e1, s2)
            }
            Expr::Mul(e1, e2) => {
                let s1 = match **e1 {
                    Expr::Add(_, _) | Expr::Sub(_, _) => format!("({})", e1),
                    _ => format!("{}", e1),
                };
                let s2 = match **e2 {
                    Expr::IntNumber(_)
                    | Expr::FixedNumber(_, _)
                    | Expr::FloatNumber(_)
                    | Expr::Variable(_)
                    | Expr::Sin(_)
                    | Expr::Cos(_) => format!("{}", e2),
                    _ => format!("({})", e2),
                };
                write!(f, "{} * {}", s1, s2)
            }
            Expr::Div(e1, e2) => {
                let s1 = match **e1 {
                    Expr::Add(_, _) | Expr::Sub(_, _) => format!("({})", e1),
                    _ => format!("{}", e1),
                };
                let s2 = match **e2 {
                    Expr::IntNumber(_)
                    | Expr::FixedNumber(_, _)
                    | Expr::FloatNumber(_)
                    | Expr::Variable(_)
                    | Expr::Sin(_)
                    | Expr::Cos(_) => format!("{}", e2),
                    _ => format!("({})", e2),
                };
                write!(f, "{} / {}", s1, s2)
            }
        }
    }
}

impl Neg for Expr {
    type Output = Expr;

    fn neg(self) -> Expr {
        Expr::Neg(Box::new(self))
    }
}

impl Visitor for Expr {
    fn sin(e: Expr) -> Expr {
        Expr::Sin(Box::new(e))
    }

    fn cos(e: Expr) -> Expr {
        Expr::Cos(Box::new(e))
    }
}

impl Add for Expr {
    type Output = Expr;

    fn add(self, other: Expr) -> Expr {
        Expr::Add(Box::new(self), Box::new(other))
    }
}

impl Sub for Expr {
    type Output = Expr;

    fn sub(self, other: Expr) -> Expr {
        Expr::Sub(Box::new(self), Box::new(other))
    }
}

impl Mul for Expr {
    type Output = Expr;

    fn mul(self, other: Expr) -> Expr {
        Expr::Mul(Box::new(self), Box::new(other))
    }
}

impl Div for Expr {
    type Output = Expr;

    fn div(self, other: Expr) -> Expr {
        Expr::Div(Box::new(self), Box::new(other))
    }
}
