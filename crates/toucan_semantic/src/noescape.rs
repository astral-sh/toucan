//! Clang parameter promises, separate from C compatibility and proven escape behavior.

use std::collections::HashMap;
use std::num::NonZeroU32;

use crate::analyze::Analyzer;
use crate::{CallingConvention, Error, TranslationUnit, Type, TypeKind};
use lang_c::span::Span;
use serde::{Serialize, Serializer};

/// Index into one translation unit's sparse parameter-contract arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ParameterContractsId(NonZeroU32);

impl ParameterContractsId {
    /// Constructs an owner-local index; the owning unit validates its existence.
    pub fn new(index: usize) -> Option<Self> {
        u32::try_from(index)
            .ok()?
            .checked_add(1)
            .and_then(NonZeroU32::new)
            .map(Self)
    }
    pub fn index(self) -> usize {
        (self.0.get() - 1) as usize
    }
}
impl Serialize for ParameterContractsId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.0.get() - 1)
    }
}

/// Promises on fixed parameters, in ascending zero-based parameter order.
///
/// `no_escape` promises that references derived from the pointer do not survive
/// the call. It does not prohibit freeing the pointer and is not an escape-analysis
/// verdict. The physical calling convention is unchanged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ParameterContracts {
    pub no_escape: Vec<u32>,
}

const MAX_SETS: usize = 65_536;
const MAX_POSITIONS: usize = 1_048_576;

pub(crate) struct ContractIndex {
    sets: HashMap<Vec<u32>, ParameterContractsId>,
    positions: usize,
}

impl TranslationUnit {
    pub fn parameter_contracts(
        &self,
        id: ParameterContractsId,
    ) -> Result<&ParameterContracts, Error> {
        self.parameter_contracts
            .get(id.index())
            .ok_or_else(|| Error::new(0, "invalid parameter-contract ID"))
    }
    pub(crate) fn same_parameter_contracts(
        &self,
        a: Option<ParameterContractsId>,
        b: Option<ParameterContractsId>,
    ) -> Result<bool, Error> {
        Ok(self.noescape_parameters(a)? == self.noescape_parameters(b)?)
    }
    pub(crate) fn noescape_parameters(
        &self,
        id: Option<ParameterContractsId>,
    ) -> Result<&[u32], Error> {
        match id {
            Some(id) => Ok(&self.parameter_contracts(id)?.no_escape),
            None => Ok(&[]),
        }
    }
    /// Validates caller-built contract tables and every structurally owned type.
    /// Nominal record/typedef edges are visited through their owning tables.
    pub fn validate_parameter_contracts(&self) -> Result<(), Error> {
        let mut positions = 0usize;
        if self.parameter_contracts.len() > MAX_SETS {
            return Err(Error::new(
                0,
                "parameter-contract set count exceeds the 65536 limit",
            ));
        }
        for set in &self.parameter_contracts {
            positions = positions
                .checked_add(set.no_escape.len())
                .ok_or_else(|| Error::new(0, "parameter-contract payload limit exceeded"))?;
            if set.no_escape.is_empty() || set.no_escape.windows(2).any(|pair| pair[0] >= pair[1]) {
                return Err(Error::new(
                    0,
                    "parameter contracts must contain sorted unique parameter positions",
                ));
            }
        }
        if positions > MAX_POSITIONS {
            return Err(Error::new(
                0,
                "parameter-contract payload exceeds the 1048576-position limit",
            ));
        }
        let mut work = 0;
        for ty in self
            .declarations
            .iter()
            .map(|d| &d.ty)
            .chain(self.typedefs.values())
            .chain(
                self.records
                    .iter()
                    .flat_map(|r| r.fields.iter().flatten().map(|f| &f.ty)),
            )
        {
            self.validate_contract_type(ty, 0, &mut work)?;
        }
        Ok(())
    }
    fn validate_contract_type(
        &self,
        ty: &Type,
        depth: usize,
        work: &mut usize,
    ) -> Result<(), Error> {
        *work += 1;
        if depth >= 128 || *work > 4_000_000 {
            return Err(Error::new(
                0,
                "parameter-contract type nesting or work limit exceeded",
            ));
        }
        match &ty.kind {
            TypeKind::Pointer(t)
            | TypeKind::Atomic(t)
            | TypeKind::Vector { element: t, .. }
            | TypeKind::Array { element: t, .. }
            | TypeKind::VariableArray { element: t, .. } => {
                self.validate_contract_type(t, depth + 1, work)?
            }
            TypeKind::Function(f) => {
                if let Some(id) = f.parameter_contracts {
                    if self.compiler != toucan_target::Compiler::Clang || !f.prototype {
                        return Err(Error::new(
                            0,
                            "parameter contracts require a Clang function prototype",
                        ));
                    }
                    for &position in &self.parameter_contracts(id)?.no_escape {
                        let p = f.parameters.get(position as usize).ok_or_else(|| {
                            Error::new(
                                0,
                                "parameter-contract position is outside the function signature",
                            )
                        })?;
                        if !matches!(self.resolve(&p.ty)?.kind, TypeKind::Pointer(_)) {
                            return Err(Error::new(
                                0,
                                "noescape parameter contract requires a pointer parameter",
                            ));
                        }
                    }
                }
                self.validate_contract_type(&f.return_type, depth + 1, work)?;
                for p in &f.parameters {
                    self.validate_contract_type(&p.ty, depth + 1, work)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn intern(
    contracts: &mut Vec<ParameterContracts>,
    index: &mut Option<Box<ContractIndex>>,
    mut checked: Option<&mut crate::checked::Builder>,
    parameters: &[u32],
    offset: usize,
) -> Result<Option<ParameterContractsId>, Error> {
    if parameters.is_empty() {
        return Ok(None);
    }
    if index.is_none() {
        *index = Some(Box::new(ContractIndex {
            positions: contracts.iter().map(|c| c.no_escape.len()).sum(),
            sets: contracts
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    (
                        c.no_escape.clone(),
                        ParameterContractsId::new(i).expect("validated contract arena"),
                    )
                })
                .collect(),
        }));
    }
    let index = index.as_mut().expect("contract index");
    if let Some(&id) = index.sets.get(parameters) {
        return Ok(Some(id));
    }
    if contracts.len() >= MAX_SETS
        || parameters.len() > MAX_POSITIONS.saturating_sub(index.positions)
    {
        return Err(Error::new(
            offset,
            "parameter-contract arena limit exceeded",
        ));
    }
    if let Some(checked) = &mut checked {
        checked.charge_parameter_contract(parameters.len(), offset)?;
    }
    let id = ParameterContractsId::new(contracts.len()).expect("bounded contract arena");
    contracts.push(ParameterContracts {
        no_escape: parameters.to_vec(),
    });
    index.sets.insert(parameters.to_vec(), id);
    index.positions += parameters.len();
    Ok(Some(id))
}

impl Analyzer {
    pub(crate) fn intern_parameter_contracts(
        &mut self,
        parameters: &[u32],
        offset: usize,
    ) -> Result<Option<ParameterContractsId>, Error> {
        intern(
            &mut self.unit.parameter_contracts,
            &mut self.parameter_contract_index,
            self.checked.as_deref_mut(),
            parameters,
            offset,
        )
    }
    pub(crate) fn check_noescape_parameter(
        &self,
        ty: &Type,
        prefix: &[(Span, bool)],
        suffix: &[(Span, bool)],
    ) -> Result<bool, Error> {
        for &(span, arguments) in prefix.iter().chain(suffix) {
            if arguments {
                return Err(Error::new(span.start, "noescape takes no arguments"));
            }
        }
        Ok((!prefix.is_empty() || !suffix.is_empty())
            && matches!(self.unit.resolve(ty)?.kind, TypeKind::Pointer(_)))
    }
}

/// Unit lookups and the mutable contract arena are disjoint during composition.
/// This avoids cloning input function signatures solely to satisfy a mutable borrow.
pub(crate) struct Composite<'a> {
    pub(crate) conditional: bool,
    pub(crate) unit: &'a TranslationUnit,
    pub(crate) contracts: &'a mut Vec<ParameterContracts>,
    pub(crate) index: &'a mut Option<Box<ContractIndex>>,
    pub(crate) checked: Option<&'a mut crate::checked::Builder>,
}
impl Composite<'_> {
    fn intersection(
        &mut self,
        a: Option<ParameterContractsId>,
        b: Option<ParameterContractsId>,
    ) -> Result<Option<ParameterContractsId>, Error> {
        let (Some(a), Some(b)) = (a, b) else {
            return Ok(None);
        };
        if a == b {
            return Ok(Some(a));
        }
        let a_set = &self
            .contracts
            .get(a.index())
            .ok_or_else(|| Error::new(0, "invalid parameter-contract ID"))?
            .no_escape;
        let b_set = &self
            .contracts
            .get(b.index())
            .ok_or_else(|| Error::new(0, "invalid parameter-contract ID"))?
            .no_escape;
        if a_set.iter().all(|v| b_set.binary_search(v).is_ok()) {
            return Ok(Some(a));
        }
        if b_set.iter().all(|v| a_set.binary_search(v).is_ok()) {
            return Ok(Some(b));
        }
        let values: Vec<_> = a_set
            .iter()
            .copied()
            .filter(|v| b_set.binary_search(v).is_ok())
            .collect();
        intern(
            self.contracts,
            self.index,
            self.checked.as_deref_mut(),
            &values,
            0,
        )
    }
    /// Combines compatible declarations without losing nested type information.
    pub(crate) fn composite_type(
        &mut self,
        left: &Type,
        right: &Type,
        depth: usize,
    ) -> Result<Type, Error> {
        if depth >= 128 {
            return Err(Error::new(
                0,
                "composite type nesting exceeds the 128-level limit",
            ));
        }
        if left == right {
            return Ok(left.clone());
        }
        let unit = self.unit;
        let resolved_left = unit.resolve(left)?;
        let resolved_right = unit.resolve(right)?;
        let kind = match (&resolved_left.kind, &resolved_right.kind) {
            (TypeKind::Pointer(a), TypeKind::Pointer(b)) => {
                TypeKind::Pointer(Box::new(self.composite_type(a, b, depth + 1)?))
            }
            (TypeKind::Atomic(a), TypeKind::Atomic(b)) => {
                TypeKind::Atomic(Box::new(self.composite_type(a, b, depth + 1)?))
            }
            (
                TypeKind::Array {
                    element: a,
                    length: a_len,
                },
                TypeKind::Array {
                    element: b,
                    length: b_len,
                },
            ) => TypeKind::Array {
                element: Box::new(self.composite_type(a, b, depth + 1)?),
                length: a_len.or(*b_len),
            },
            (
                TypeKind::Array {
                    element: a,
                    length: Some(length),
                },
                TypeKind::VariableArray { element: b, .. },
            )
            | (
                TypeKind::VariableArray { element: a, .. },
                TypeKind::Array {
                    element: b,
                    length: Some(length),
                },
            ) => TypeKind::Array {
                element: Box::new(self.composite_type(a, b, depth + 1)?),
                length: Some(*length),
            },
            (
                TypeKind::VariableArray {
                    element: a,
                    identity,
                },
                TypeKind::VariableArray { element: b, .. },
            )
            | (
                TypeKind::VariableArray {
                    element: a,
                    identity,
                },
                TypeKind::Array {
                    element: b,
                    length: None,
                },
            )
            | (
                TypeKind::Array {
                    element: a,
                    length: None,
                },
                TypeKind::VariableArray {
                    element: b,
                    identity,
                },
            ) => TypeKind::VariableArray {
                element: Box::new(self.composite_type(a, b, depth + 1)?),
                identity: *identity,
            },
            (TypeKind::Function(a), TypeKind::Function(b)) => {
                let mut function = if a.prototype {
                    (**a).clone()
                } else {
                    (**b).clone()
                };
                function.noreturn = if self.conditional {
                    a.noreturn && b.noreturn
                } else {
                    a.noreturn || b.noreturn
                };
                if function.calling_convention == CallingConvention::C {
                    function.calling_convention = if a.calling_convention != CallingConvention::C {
                        a.calling_convention
                    } else {
                        b.calling_convention
                    };
                }
                function.return_type =
                    self.composite_type(&a.return_type, &b.return_type, depth + 1)?;
                if a.prototype && b.prototype {
                    function.parameter_contracts =
                        self.intersection(a.parameter_contracts, b.parameter_contracts)?;
                    for ((parameter, a), b) in function
                        .parameters
                        .iter_mut()
                        .zip(&a.parameters)
                        .zip(&b.parameters)
                    {
                        parameter.ty = match (
                            self.unit.transparent_union(&a.ty)?,
                            self.unit.transparent_union(&b.ty)?,
                        ) {
                            (Some(_), None) => b.ty.clone(),
                            (None, Some(_)) => a.ty.clone(),
                            _ => self.composite_type(&a.ty, &b.ty, depth + 1)?,
                        };
                    }
                }
                TypeKind::Function(Box::new(function))
            }
            _ => return Ok(left.clone()),
        };
        if kind == resolved_left.kind {
            return Ok(left.clone());
        }
        Ok(Type {
            kind,
            qualifiers: self.unit.qualifiers(left)?,
            alignment: self.unit.typedef_alignment(left)?,
        })
    }
}

// Expanding at the call site preserves disjoint field borrows when an input type
// lives in the unit or a lexical scope. A whole-Analyzer mutable method would
// require cloning those signatures before each ordinary redeclaration.
macro_rules! composite_type {
    ($analyzer:ident, $left:expr, $right:expr, $depth:expr) => {
        $crate::noescape::composite_type!($analyzer, $left, $right, $depth, false)
    };
    ($analyzer:ident, $left:expr, $right:expr, $depth:expr, $conditional:expr) => {{
        let mut contracts = std::mem::take(&mut $analyzer.unit.parameter_contracts);
        let result = $crate::noescape::Composite {
            conditional: $conditional,
            unit: &$analyzer.unit,
            contracts: &mut contracts,
            index: &mut $analyzer.parameter_contract_index,
            checked: $analyzer.checked.as_deref_mut(),
        }
        .composite_type($left, $right, $depth);
        $analyzer.unit.parameter_contracts = contracts;
        result
    }};
}
pub(crate) use composite_type;

impl Analyzer {
    /// Clang's C rule rejects the reverse of a contract-dropping function
    /// conversion. Crossing masks and nested callback differences remain C
    /// compatible; this is deliberately not a general promise-subtyping rule.
    pub(crate) fn check_noescape_conversion(
        &self,
        destination: &Type,
        source: &Type,
        offset: usize,
    ) -> Result<(), Error> {
        if self.unit.compiler != toucan_target::Compiler::Clang {
            return Ok(());
        }
        // The compiler applies this rule to the immediate pointees, and its
        // function-conversion helper strips at most one additional pointer.
        let (destination, source) = match (&destination.kind, &source.kind) {
            (TypeKind::Pointer(to), TypeKind::Pointer(from)) => {
                (self.unit.resolve(to)?, self.unit.resolve(from)?)
            }
            _ => (destination, source),
        };
        let (TypeKind::Function(to), TypeKind::Function(from)) = (&destination.kind, &source.kind)
        else {
            return Ok(());
        };
        let to_set = self.unit.noescape_parameters(to.parameter_contracts)?;
        if to_set.is_empty() && !to.noreturn {
            return Ok(());
        }
        let from_set = self.unit.noescape_parameters(from.parameter_contracts)?;
        if (from.noreturn && !to.noreturn)
            || !from_set
                .iter()
                .all(|position| to_set.binary_search(position).is_ok())
            || (to_set == from_set && to.noreturn == from.noreturn)
        {
            return Ok(());
        }
        let mut adjusted = source.clone();
        let TypeKind::Function(from) = &mut adjusted.kind else {
            unreachable!()
        };
        from.parameter_contracts = to.parameter_contracts;
        from.noreturn = to.noreturn;
        if self.same_type(destination, &adjusted, 0)? {
            return Err(Error::new(
                offset,
                "function pointer conversion strengthens a noescape or noreturn contract",
            ));
        }
        Ok(())
    }
}

impl Analyzer {
    /// GNU spelling changes Clang's first wrapped function type. Scalar and
    /// atomic subjects ignore it; parameters/return types are not traversed.
    pub(crate) fn apply_type_noreturn(&self, mut ty: Type, depth: usize) -> Result<Type, Error> {
        let mut current = &ty;
        let mut steps = depth;
        loop {
            if steps >= 128 {
                return Err(Error::new(
                    0,
                    "noreturn type nesting exceeds the 128-level limit",
                ));
            }
            current = self.unit.resolve(current)?;
            match &current.kind {
                TypeKind::Function(f) if !f.noreturn => break,
                TypeKind::Pointer(t)
                | TypeKind::Array { element: t, .. }
                | TypeKind::VariableArray { element: t, .. } => current = t,
                _ => return Ok(ty),
            }
            steps += 1;
        }
        self.set_type_noreturn(&mut ty, depth)?;
        Ok(ty)
    }
    fn set_type_noreturn(&self, ty: &mut Type, depth: usize) -> Result<(), Error> {
        if depth >= 128 {
            return Err(Error::new(
                0,
                "noreturn type nesting exceeds the 128-level limit",
            ));
        }
        if matches!(ty.kind, TypeKind::Typedef(_)) {
            let qualifiers = self.unit.qualifiers(ty)?;
            let alignment = self.unit.typedef_alignment(ty)?;
            *ty = self.unit.resolve(ty)?.clone();
            ty.qualifiers = qualifiers;
            ty.alignment = alignment;
        }
        match &mut ty.kind {
            TypeKind::Function(f) => f.noreturn = true,
            TypeKind::Pointer(t)
            | TypeKind::Array { element: t, .. }
            | TypeKind::VariableArray { element: t, .. } => self.set_type_noreturn(t, depth + 1)?,
            _ => {}
        }
        Ok(())
    }
}

pub(crate) fn has_type_noreturn(unit: &TranslationUnit) -> bool {
    fn contains(ty: &Type, depth: usize) -> bool {
        if depth >= 128 {
            return false;
        }
        match &ty.kind {
            TypeKind::Function(f) => {
                f.noreturn
                    || contains(&f.return_type, depth + 1)
                    || f.parameters.iter().any(|p| contains(&p.ty, depth + 1))
            }
            TypeKind::Pointer(t)
            | TypeKind::Atomic(t)
            | TypeKind::Array { element: t, .. }
            | TypeKind::VariableArray { element: t, .. } => contains(t, depth + 1),
            _ => false,
        }
    }
    unit.declarations
        .iter()
        .map(|d| &d.ty)
        .chain(unit.typedefs.values())
        .chain(
            unit.records
                .iter()
                .flat_map(|r| r.fields.iter().flatten().map(|f| &f.ty)),
        )
        .any(|ty| contains(ty, 0))
}
