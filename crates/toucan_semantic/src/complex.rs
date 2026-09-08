//! Target complex arithmetic. Real operands retain their domain; finite GNU
//! products/quotients are returned only when directed bounds prove their rounding.

use lang_c::ast::BinaryOperator as Op;
use rustc_apfloat::ieee::Quad;
use rustc_apfloat::{Float, FloatConvert, Round};

use crate::Error;

/// Evaluate components at the corresponding real precision. Clang's full-complex
/// formulas use target rounding and denominator scaling, including infinity
/// recovery; mixed-real formulas omit nonexistent imaginary operations.
pub(crate) fn binary<F>(
    operator: &Op,
    parts: [Quad; 4],
    real_domains: [bool; 2],
    gnu: bool,
    offset: usize,
) -> Result<[Quad; 2], Error>
where
    F: Float + FloatConvert<Quad>,
    Quad: FloatConvert<F>,
{
    let [a, b, c, d] = parts.map(|x| x.convert(&mut false).value);
    let [left_real, right_real] = real_domains;
    if gnu
        && !right_real
        && (!left_real || *operator == Op::Divide)
        && matches!(operator, Op::Multiply | Op::Divide)
        && parts.iter().all(|value| value.is_finite())
        && (*operator != Op::Divide || !c.is_zero() || !d.is_zero())
    {
        return finite_gnu::<F>(operator, parts, offset);
    }
    let [mut real, mut imaginary] = match operator {
        Op::Plus => [
            add(a, c),
            if left_real {
                d
            } else if right_real {
                b
            } else {
                add(b, d)
            },
        ],
        Op::Minus => [
            sub(a, c),
            if left_real {
                -d
            } else if right_real {
                b
            } else {
                sub(b, d)
            },
        ],
        Op::Multiply if left_real => [mul(a, c), mul(a, d)],
        Op::Multiply if right_real => [mul(c, a), mul(c, b)],
        Op::Multiply => multiply(a, b, c, d),
        Op::Divide if right_real => [div(a, c), div(b, c)],
        Op::Divide => divide(a, b, c, d),
        _ => {
            return Err(Error::new(
                offset,
                "operator does not accept complex operands",
            ));
        }
    };
    // GCC folds full-complex multiplication/division through MPC/MPFR,
    // whose NaN representation has no payload or sign. Mixed-real formulas
    // remain ordinary real operations and preserve their input NaN payloads.
    if gnu
        && !right_real
        && (!left_real || *operator == Op::Divide)
        && matches!(operator, Op::Multiply | Op::Divide)
    {
        if real.is_nan() {
            real = F::NAN;
        }
        if imaginary.is_nan() {
            imaginary = F::NAN;
        }
    }
    Ok([
        real.convert(&mut false).value,
        imaginary.convert(&mut false).value,
    ])
}

fn add<F: Float>(a: F, b: F) -> F {
    a.add_r(b, Round::NearestTiesToEven).value
}
fn sub<F: Float>(a: F, b: F) -> F {
    a.sub_r(b, Round::NearestTiesToEven).value
}
fn mul<F: Float>(a: F, b: F) -> F {
    a.mul_r(b, Round::NearestTiesToEven).value
}
fn div<F: Float>(a: F, b: F) -> F {
    a.div_r(b, Round::NearestTiesToEven).value
}

fn boxed_infinity<F: Float>(value: F) -> F {
    if value.is_infinite() {
        F::from_u128(1).value.copy_sign(value)
    } else {
        F::ZERO.copy_sign(value)
    }
}
fn nan_to_zero<F: Float>(value: F) -> F {
    if value.is_nan() {
        F::ZERO.copy_sign(value)
    } else {
        value
    }
}

fn multiply<F: Float>(mut a: F, mut b: F, mut c: F, mut d: F) -> [F; 2] {
    let [ac, bd, ad, bc] = [mul(a, c), mul(b, d), mul(a, d), mul(b, c)];
    let result = [sub(ac, bd), add(ad, bc)];
    if !result.iter().all(|value| value.is_nan()) {
        return result;
    }
    let mut recalculate = false;
    if a.is_infinite() || b.is_infinite() {
        a = boxed_infinity(a);
        b = boxed_infinity(b);
        c = nan_to_zero(c);
        d = nan_to_zero(d);
        recalculate = true;
    }
    if c.is_infinite() || d.is_infinite() {
        c = boxed_infinity(c);
        d = boxed_infinity(d);
        a = nan_to_zero(a);
        b = nan_to_zero(b);
        recalculate = true;
    }
    if !recalculate && [ac, bd, ad, bc].iter().any(|value| value.is_infinite()) {
        a = nan_to_zero(a);
        b = nan_to_zero(b);
        c = nan_to_zero(c);
        d = nan_to_zero(d);
        recalculate = true;
    }
    if recalculate {
        [
            mul(F::INFINITY, sub(mul(a, c), mul(b, d))),
            mul(F::INFINITY, add(mul(a, d), mul(b, c))),
        ]
    } else {
        result
    }
}

fn divide<F: Float>(mut a: F, mut b: F, mut c: F, mut d: F) -> [F; 2] {
    let maximum = if c.is_nan() {
        d.abs()
    } else if d.is_nan() || c.abs() > d.abs() {
        c.abs()
    } else {
        d.abs()
    };
    // ilogb(0) is a sentinel, not an exponent that can safely be negated.
    let exponent = if maximum.is_finite_non_zero() {
        maximum.ilogb()
    } else {
        0
    };
    if maximum.is_finite_non_zero() {
        c = c.scalbn(-exponent);
        d = d.scalbn(-exponent);
    }
    let denominator = add(mul(c, c), mul(d, d));
    let result = [
        div(add(mul(a, c), mul(b, d)), denominator).scalbn(-exponent),
        div(sub(mul(b, c), mul(a, d)), denominator).scalbn(-exponent),
    ];
    if !result.iter().all(|value| value.is_nan()) {
        return result;
    }
    if denominator.is_zero() && (!a.is_nan() || !b.is_nan()) {
        let infinity = F::INFINITY.copy_sign(c);
        [mul(infinity, a), mul(infinity, b)]
    } else if (a.is_infinite() || b.is_infinite()) && c.is_finite() && d.is_finite() {
        a = boxed_infinity(a);
        b = boxed_infinity(b);
        [
            mul(F::INFINITY, add(mul(a, c), mul(b, d))),
            mul(F::INFINITY, sub(mul(b, c), mul(a, d))),
        ]
    } else if maximum.is_infinite() && a.is_finite() && b.is_finite() {
        c = boxed_infinity(c);
        d = boxed_infinity(d);
        [
            mul(F::ZERO, add(mul(a, c), mul(b, d))),
            mul(F::ZERO, sub(mul(b, c), mul(a, d))),
        ]
    } else {
        result
    }
}

/// Each endpoint encloses exact real arithmetic. Binary128 adds enough precision
/// and exponent range for most float/double cases; uncertainty is an explicit
/// diagnostic, including hard rounding boundaries and extended-format extremes.
#[derive(Clone, Copy)]
struct Interval {
    lower: Quad,
    upper: Quad,
    nearest: Quad,
}
impl Interval {
    fn point(value: Quad) -> Self {
        Self {
            lower: value,
            upper: value,
            nearest: value,
        }
    }
    fn add(self, other: Self) -> Self {
        Self {
            lower: self.lower.add_r(other.lower, Round::TowardNegative).value,
            upper: self.upper.add_r(other.upper, Round::TowardPositive).value,
            nearest: add(self.nearest, other.nearest),
        }
    }
    fn sub(self, other: Self) -> Self {
        Self {
            lower: self.lower.sub_r(other.upper, Round::TowardNegative).value,
            upper: self.upper.sub_r(other.lower, Round::TowardPositive).value,
            nearest: sub(self.nearest, other.nearest),
        }
    }
    fn product(self, other: Self, divide: bool) -> Option<Self> {
        if divide && other.lower <= Quad::ZERO && other.upper >= Quad::ZERO {
            return None;
        }
        let mut lower = Quad::INFINITY;
        let mut upper = -Quad::INFINITY;
        for a in [self.lower, self.upper] {
            for b in [other.lower, other.upper] {
                let operation = |round| {
                    if divide {
                        a.div_r(b, round).value
                    } else {
                        a.mul_r(b, round).value
                    }
                };
                let lo = operation(Round::TowardNegative);
                let hi = operation(Round::TowardPositive);
                if lo.is_nan() || hi.is_nan() {
                    return None;
                }
                if lo < lower {
                    lower = lo;
                }
                if hi > upper {
                    upper = hi;
                }
            }
        }
        Some(Self {
            lower,
            upper,
            nearest: if divide {
                div(self.nearest, other.nearest)
            } else {
                mul(self.nearest, other.nearest)
            },
        })
    }
    fn rounded<F>(self) -> Option<Quad>
    where
        Quad: FloatConvert<F>,
        F: Float + FloatConvert<Quad>,
    {
        if self.lower.is_nan() || self.upper.is_nan() {
            return None;
        }
        let lower: F = self.lower.convert(&mut false).value;
        let upper: F = self.upper.convert(&mut false).value;
        if lower.is_zero() && upper.is_zero() && self.lower.is_zero() && self.upper.is_zero() {
            return Some(Quad::ZERO.copy_sign(self.nearest));
        }
        (lower.to_bits() == upper.to_bits()).then(|| lower.convert(&mut false).value)
    }
}

fn finite_gnu<F>(operator: &Op, parts: [Quad; 4], offset: usize) -> Result<[Quad; 2], Error>
where
    F: Float + FloatConvert<Quad>,
    Quad: FloatConvert<F>,
{
    let fail = || {
        Error::new(
            offset,
            "GNU complex constant rounding could not be proven at the supported precision",
        )
    };
    let [a, b, c, d] = parts.map(Interval::point);
    let (real, imaginary) = if *operator == Op::Multiply {
        (
            a.product(c, false)
                .ok_or_else(fail)?
                .sub(b.product(d, false).ok_or_else(fail)?),
            a.product(d, false)
                .ok_or_else(fail)?
                .add(b.product(c, false).ok_or_else(fail)?),
        )
    } else {
        let denominator = c
            .product(c, false)
            .ok_or_else(fail)?
            .add(d.product(d, false).ok_or_else(fail)?);
        (
            a.product(c, false)
                .ok_or_else(fail)?
                .add(b.product(d, false).ok_or_else(fail)?)
                .product(denominator, true)
                .ok_or_else(fail)?,
            b.product(c, false)
                .ok_or_else(fail)?
                .sub(a.product(d, false).ok_or_else(fail)?)
                .product(denominator, true)
                .ok_or_else(fail)?,
        )
    };
    Ok([
        real.rounded::<F>().ok_or_else(fail)?,
        imaginary.rounded::<F>().ok_or_else(fail)?,
    ])
}

impl crate::analyze::Analyzer {
    /// GCC preserves ordinary complex operand qualifiers on unary value results.
    pub(crate) fn complex_unary_result(
        &self,
        operand: &crate::expression::ExpressionInfo,
        value: crate::Type,
    ) -> Result<crate::Type, Error> {
        if self.gnu_sync_profile()
            && matches!(value.kind, crate::TypeKind::Complex(_))
            && self.unit.atomic_value(&operand.ty)?.is_none()
        {
            Ok(operand.ty.clone())
        } else {
            Ok(value)
        }
    }

    /// GNU projections preserve a source place without inventing record fields.
    pub(crate) fn complex_projection(
        &self,
        mut operand: crate::expression::ExpressionInfo,
        imaginary: bool,
        offset: usize,
    ) -> Result<crate::expression::ExpressionInfo, Error> {
        use crate::expression::ExpressionInfo;
        use crate::{Type, TypeKind};
        if self.unit.atomic_value(&operand.ty)?.is_some() {
            return Err(Error::new(
                offset,
                if self.gnu_sync_profile() {
                    "GNU projection of atomic storage requires unsupported component-access semantics"
                } else {
                    "the Clang profile rejects projection of an atomic operand"
                },
            ));
        }
        let ty = self.unit.resolve(&operand.ty)?;
        if let TypeKind::Complex(kind) = ty.kind {
            operand.volatile_place |= self.unit.qualifiers(&operand.ty)?.is_volatile;
            operand.ty = Type::new(TypeKind::Float(kind));
            operand.alignment_origin = None;
            return Ok(operand);
        }
        self.require_arithmetic(&operand.ty, offset)?;
        if imaginary && operand.bitfield.is_some() && self.gnu_sync_profile() {
            return Err(Error::new(
                offset,
                "GNU imaginary projection of a bitfield requires an unsupported precise-width integer result",
            ));
        }
        if !self.gnu_sync_profile() && (operand.bitfield.is_some() || operand.vector_element) {
            return Ok(ExpressionInfo::value(
                self.converted_type(&operand, offset)?,
            ));
        }
        if imaginary {
            return Ok(ExpressionInfo::value(operand.ty));
        }
        if !self.gnu_sync_profile() {
            operand.alignment_origin = None;
        }
        Ok(operand)
    }
}
