use crate::symrs::expr::Expr;

#[test]
pub fn test_display() {
    let e = Expr::IntNumber(1);
    assert_eq!(format!("{}", e), "1");

    let e = Expr::FixedNumber(1, 2);
    assert_eq!(format!("{}", e), "1.2");

    let e = Expr::FloatNumber(1.2);
    assert_eq!(format!("{}", e), "1.2");

    let e = Expr::Variable("x".to_string());
    assert_eq!(format!("{}", e), "x");

    let e = Expr::Neg(Box::new(Expr::IntNumber(1)));
    assert_eq!(format!("{}", e), "-1");

    let e = Expr::Sin(Box::new(Expr::IntNumber(1)));
    assert_eq!(format!("{}", e), "sin(1)");

    let e = Expr::Cos(Box::new(Expr::IntNumber(1)));
    assert_eq!(format!("{}", e), "cos(1)");

    let e = Expr::Add(Box::new(Expr::IntNumber(1)), Box::new(Expr::IntNumber(2)));
    assert_eq!(format!("{}", e), "1 + 2");

    let e = Expr::Sub(Box::new(Expr::IntNumber(1)), Box::new(Expr::IntNumber(2)));
    assert_eq!(format!("{}", e), "1 - 2");

    let e = Expr::Mul(Box::new(Expr::IntNumber(1)), Box::new(Expr::IntNumber(2)));
    assert_eq!(format!("{}", e), "1 * 2");

    let e = Expr::Div(Box::new(Expr::IntNumber(1)), Box::new(Expr::IntNumber(2)));
    assert_eq!(format!("{}", e), "1 / 2");

    let e = Expr::Add(
        Box::new(Expr::IntNumber(1)),
        Box::new(Expr::Mul(
            Box::new(Expr::IntNumber(2)),
            Box::new(Expr::IntNumber(3)),
        )),
    );
    assert_eq!(format!("{}", e), "1 + 2 * 3");

    let e = Expr::Mul(
        Box::new(Expr::Add(
            Box::new(Expr::IntNumber(1)),
            Box::new(Expr::IntNumber(2)),
        )),
        Box::new(Expr::IntNumber(3)),
    );
    assert_eq!(format!("{}", e), "(1 + 2) * 3");

    let e = Expr::Mul(
        Box::new(Expr::IntNumber(1)),
        Box::new(Expr::Add(
            Box::new(Expr::IntNumber(2)),
            Box::new(Expr::IntNumber(3)),
        )),
    );
    assert_eq!(format!("{}", e), "1 * (2 + 3)");
}

#[test]
pub fn test_neg() {
    let e = Expr::IntNumber(1);
    let e = -e;
    assert_eq!(format!("{}", e), "-1");

    let e = Expr::FixedNumber(1, 2);
    let e = -e;
    assert_eq!(format!("{}", e), "-1.2");

    let e = Expr::FloatNumber(1.2);
    let e = -e;
    assert_eq!(format!("{}", e), "-1.2");

    let e = Expr::Variable("x".to_string());
    let e = -e;
    assert_eq!(format!("{}", e), "-x");

    let e = Expr::Neg(Box::new(Expr::IntNumber(1)));
    let e = -e;
    assert_eq!(format!("{}", e), "-(-1)");

    let e = Expr::Sin(Box::new(Expr::IntNumber(1)));
    let e = -e;
    assert_eq!(format!("{}", e), "-sin(1)");

    let e = Expr::Cos(Box::new(Expr::IntNumber(1)));
    let e = -e;
    assert_eq!(format!("{}", e), "-cos(1)");

    let e = Expr::Add(Box::new(Expr::IntNumber(1)), Box::new(Expr::IntNumber(2)));
    let e = -e;
    assert_eq!(format!("{}", e), "-(1 + 2)");

    let e = Expr::Sub(Box::new(Expr::IntNumber(1)), Box::new(Expr::IntNumber(2)));
    let e = -e;
    assert_eq!(format!("{}", e), "-(1 - 2)");

    let e = Expr::Mul(Box::new(Expr::IntNumber(1)), Box::new(Expr::IntNumber(2)));
    let e = -e;
    assert_eq!(format!("{}", e), "-(1 * 2)");

    let e = Expr::Div(Box::new(Expr::IntNumber(1)), Box::new(Expr::IntNumber(2)));
    let e = -e;
    assert_eq!(format!("{}", e), "-(1 / 2)");
}

#[test]
pub fn test_sin() {
    let e = Expr::IntNumber(1);
    let e = Expr::Sin(Box::new(e));
    assert_eq!(format!("{}", e), "sin(1)");

    let e = Expr::FixedNumber(1, 2);
    let e = Expr::Sin(Box::new(e));
    assert_eq!(format!("{}", e), "sin(1.2)");

    let e = Expr::FloatNumber(1.2);
    let e = Expr::Sin(Box::new(e));
    assert_eq!(format!("{}", e), "sin(1.2)");

    let e = Expr::Variable("x".to_string());
    let e = Expr::Sin(Box::new(e));
    assert_eq!(format!("{}", e), "sin(x)");

    let e = Expr::Neg(Box::new(Expr::IntNumber(1)));
    let e = Expr::Sin(Box::new(e));
    assert_eq!(format!("{}", e), "sin(-1)");

    let e = Expr::Sin(Box::new(Expr::IntNumber(1)));
    let e = Expr::Sin(Box::new(e));
    assert_eq!(format!("{}", e), "sin(sin(1))");

    let e = Expr::Cos(Box::new(Expr::IntNumber(1)));
    let e = Expr::Sin(Box::new(e));
    assert_eq!(format!("{}", e), "sin(cos(1))");

    let e = Expr::Add(Box::new(Expr::IntNumber(1)), Box::new(Expr::IntNumber(2)));
    let e = Expr::Sin(Box::new(e));
    assert_eq!(format!("{}", e), "sin(1 + 2)");

    let e = Expr::Sub(Box::new(Expr::IntNumber(1)), Box::new(Expr::IntNumber(2)));
    let e = Expr::Sin(Box::new(e));
    assert_eq!(format!("{}", e), "sin(1 - 2)");

    let e = Expr::Mul(Box::new(Expr::IntNumber(1)), Box::new(Expr::IntNumber(2)));
    let e = Expr::Sin(Box::new(e));
    assert_eq!(format!("{}", e), "sin(1 * 2)");

    let e = Expr::Div(Box::new(Expr::IntNumber(1)), Box::new(Expr::IntNumber(2)));
    let e = Expr::Sin(Box::new(e));
    assert_eq!(format!("{}", e), "sin(1 / 2)");
}

#[test]
pub fn test_add() {
    let e1 = Expr::IntNumber(1);
    let e2 = Expr::IntNumber(2);
    let e = e1 + e2;
    assert_eq!(format!("{}", e), "1 + 2");

    let e1 = Expr::IntNumber(1);
    let e2 = Expr::Add(Box::new(Expr::IntNumber(2)), Box::new(Expr::IntNumber(3)));
    let e = e1 + e2;
    assert_eq!(format!("{}", e), "1 + (2 + 3)");

    let e1 = Expr::Add(Box::new(Expr::IntNumber(1)), Box::new(Expr::IntNumber(2)));
    let e2 = Expr::IntNumber(3);
    let e = e1 + e2;
    assert_eq!(format!("{}", e), "1 + 2 + 3");

    let e1 = Expr::Add(Box::new(Expr::IntNumber(1)), Box::new(Expr::IntNumber(2)));
    let e2 = Expr::Add(Box::new(Expr::IntNumber(3)), Box::new(Expr::IntNumber(4)));
    let e = e1 + e2;
    assert_eq!(format!("{}", e), "1 + 2 + (3 + 4)");
}

#[test]
pub fn test_mul() {
    let e1 = Expr::IntNumber(1);
    let e2 = Expr::IntNumber(2);
    let e = e1 * e2;
    assert_eq!(format!("{}", e), "1 * 2");

    let e1 = Expr::IntNumber(1);
    let e2 = Expr::Add(Box::new(Expr::IntNumber(2)), Box::new(Expr::IntNumber(3)));
    let e = e1 * e2;
    assert_eq!(format!("{}", e), "1 * (2 + 3)");

    let e1 = Expr::Add(Box::new(Expr::IntNumber(1)), Box::new(Expr::IntNumber(2)));
    let e2 = Expr::IntNumber(3);
    let e = e1 * e2;
    assert_eq!(format!("{}", e), "(1 + 2) * 3");

    let e1 = Expr::Add(Box::new(Expr::IntNumber(1)), Box::new(Expr::IntNumber(2)));
    let e2 = Expr::Add(Box::new(Expr::IntNumber(3)), Box::new(Expr::IntNumber(4)));
    let e = e1 * e2;
    assert_eq!(format!("{}", e), "(1 + 2) * (3 + 4)");
}
