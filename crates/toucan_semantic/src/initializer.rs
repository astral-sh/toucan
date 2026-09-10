use lang_c::{
    ast,
    span::{Node, Span},
};

use crate::analyze::Analyzer;
use crate::checked::InitializerId;
use crate::checked::initializer::{Origin, Path as RetainedPath, Subobject};
use crate::{
    DeclarationKind, Error, FlexibleArrayStorage, IntegerKind, RecordKind, Type, TypeKind,
};

#[derive(Clone, Copy, PartialEq)]
enum ConstantKind {
    Arithmetic,
    Address,
}

/// A relocation base or an absolute target address. Symbol spelling is resolved
/// in the active lexical scope; anonymous objects use their source occurrence.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PointerBase<'a> {
    Symbol(&'a str),
    Anonymous(usize, usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct PointerConstant<'a> {
    base: Option<PointerBase<'a>>,
    offset: u128,
    nullable: bool,
}

impl PointerConstant<'_> {
    fn truth(self) -> Option<bool> {
        if self.base.is_none() {
            Some(self.offset != 0)
        } else if self.nullable {
            None
        } else {
            Some(true)
        }
    }
}

enum ConstantBranch<'a> {
    Reused(Option<PointerConstant<'a>>),
    Selected(&'a Node<ast::Expression>),
}

struct FlexibleState {
    member_index: usize,
    elements: Option<u64>,
    permitted: bool,
}

#[derive(Clone, Copy)]
enum InitializerView<'a> {
    Expression(&'a Node<ast::Expression>),
    List(&'a [Node<ast::InitializerListItem>]),
}

#[derive(Clone, Copy)]
struct InitializerRef<'a> {
    origin: Origin<'a>,
    node: InitializerView<'a>,
    span: Span,
}

impl<'a> From<&'a Node<ast::Initializer>> for InitializerRef<'a> {
    fn from(initializer: &'a Node<ast::Initializer>) -> Self {
        Self {
            origin: Origin::Written(initializer),
            node: match &initializer.node {
                ast::Initializer::Expression(expression) => InitializerView::Expression(expression),
                ast::Initializer::List(items) => InitializerView::List(items),
            },
            span: initializer.span,
        }
    }
}

impl Analyzer {
    pub(crate) fn initialize_declaration(
        &mut self,
        index: usize,
        original: &Type,
        initializer: &Node<ast::Initializer>,
        prechecked: Option<(Type, Option<FlexibleArrayStorage>)>,
    ) -> Result<(), Error> {
        if self.unit.declarations[index].kind != DeclarationKind::Variable {
            return Err(Error::new(
                initializer.span.start,
                "only an object can have an initializer",
            ));
        }
        let (completed, storage) = match prechecked {
            Some(result) => result,
            None => self.check_object_initializer(original, initializer, true)?,
        };
        self.unit.declarations[index].flexible_array_storage = storage;
        let previous = &self.unit.declarations[index].ty;
        if !self.compatible(previous, &completed)? {
            return Err(Error::new(
                initializer.span.start,
                "initializer array bound conflicts with the previous declaration",
            ));
        }
        let ty = crate::noescape::composite_type!(self, previous, &completed, 0)?;
        self.unit.declarations[index].ty = ty;
        self.unit.declarations[index].is_definition = true;
        Ok(())
    }

    /// Checks C initialization and completes an outer array whose bound is omitted.
    /// The subobject cursor visits explicit initializers only, so a large sparse
    /// designator never allocates one entry for every implicitly zeroed element.
    pub(crate) fn check_initializer(
        &mut self,
        ty: &Type,
        initializer: &Node<ast::Initializer>,
        requires_constant: bool,
    ) -> Result<Type, Error> {
        self.enter_expression(initializer.span.start)?;
        let result = self.initializer_inner(ty, initializer.into(), requires_constant, None);
        self.leave_expression();
        result
    }

    /// Compound literals borrow their written list, preserving occurrence identity
    /// and avoiding a fresh copy each time their type is queried.
    pub(crate) fn check_initializer_list(
        &mut self,
        ty: &Type,
        items: &[Node<ast::InitializerListItem>],
        owner: &Node<ast::Expression>,
        span: Span,
        requires_constant: bool,
    ) -> Result<Type, Error> {
        self.enter_expression(span.start)?;
        let result = self.initializer_inner(
            ty,
            InitializerRef {
                origin: Origin::Compound(owner),
                node: InitializerView::List(items),
                span,
            },
            requires_constant,
            None,
        );
        self.leave_expression();
        result
    }

    /// A named object's flexible member can own storage beyond its record type.
    /// Nested initializers and compound literals use `check_initializer` instead.
    pub(crate) fn check_object_initializer(
        &mut self,
        ty: &Type,
        initializer: &Node<ast::Initializer>,
        requires_constant: bool,
    ) -> Result<(Type, Option<FlexibleArrayStorage>), Error> {
        let TypeKind::Record(id) = self.unit.resolve(ty)?.kind else {
            return self
                .check_initializer(ty, initializer, requires_constant)
                .map(|ty| (ty, None));
        };
        let Some(fields) = self.unit.records[id].fields.as_ref() else {
            return self
                .check_initializer(ty, initializer, requires_constant)
                .map(|ty| (ty, None));
        };
        let Some((member_index, field)) = fields.iter().enumerate().next_back() else {
            return self
                .check_initializer(ty, initializer, requires_constant)
                .map(|ty| (ty, None));
        };
        let TypeKind::Array {
            element,
            length: None,
        } = &self.unit.resolve(&field.ty)?.kind
        else {
            return self
                .check_initializer(ty, initializer, requires_constant)
                .map(|ty| (ty, None));
        };
        let element = (**element).clone();
        let mut flexible = FlexibleState {
            member_index,
            elements: None,
            permitted: requires_constant,
        };
        self.enter_expression(initializer.span.start)?;
        let result = self.initializer_inner(
            ty,
            initializer.into(),
            requires_constant,
            Some(&mut flexible),
        );
        self.leave_expression();
        let ty = result?;
        let storage = flexible
            .elements
            .map(|elements| {
                let layout = self.unit.layout(&ty)?;
                let field_offset = layout.fields[member_index]
                    .as_ref()
                    .expect("flexible member is addressable")
                    .offset_bits;
                let tail_bits = elements
                    .checked_mul(self.unit.layout(&element)?.size_bits)
                    .ok_or_else(|| {
                        Error::new(
                            initializer.span.start,
                            "flexible array allocation size overflows",
                        )
                    })?;
                let base = if self.gnu_flexible_arrays() {
                    layout.size_bits
                } else {
                    field_offset
                };
                let size_bits = base
                    .checked_add(tail_bits)
                    .ok_or_else(|| {
                        Error::new(
                            initializer.span.start,
                            "flexible array allocation size overflows",
                        )
                    })?
                    .max(layout.size_bits);
                Ok(FlexibleArrayStorage {
                    member_index,
                    elements,
                    size_bits,
                })
            })
            .transpose()?;
        Ok((ty, storage))
    }

    fn gnu_flexible_arrays(&self) -> bool {
        self.unit.compiler == toucan_target::Compiler::Gnu
    }

    fn empty_initializer(&self, initializer: &Node<ast::Initializer>) -> bool {
        matches!(&initializer.node, ast::Initializer::List(items) if items.is_empty())
    }

    /// Finds the innermost flexible member crossed by a subobject designator.
    fn flexible_in_path(
        &self,
        root: &Type,
        path: &[u64],
        offset: usize,
    ) -> Result<Option<usize>, Error> {
        let mut ty = root.clone();
        let mut flexible = None;
        for (index, part) in path.iter().enumerate() {
            let record = matches!(self.unit.resolve(&ty)?.kind, TypeKind::Record(_));
            ty = self.subobject(&ty, std::slice::from_ref(part), offset)?;
            if record
                && matches!(
                    self.unit.resolve(&ty)?.kind,
                    TypeKind::Array { length: None, .. }
                )
            {
                flexible = Some(index + 1);
            }
        }
        Ok(flexible)
    }

    fn initializer_inner(
        &mut self,
        ty: &Type,
        initializer: InitializerRef<'_>,
        requires_constant: bool,
        flexible: Option<&mut FlexibleState>,
    ) -> Result<Type, Error> {
        let retained = self
            .checked
            .as_deref_mut()
            .map(|checked| checked.begin_initializer(initializer.origin, ty, requires_constant))
            .transpose()?
            .flatten();
        let result = if let Some(value) = self.unit.atomic_value(ty)?.cloned() {
            if !self.gnu_sync_profile() && matches!(initializer.node, InitializerView::List(_)) {
                return Err(Error::new(
                    initializer.span.start,
                    "this Clang profile requires an atomic initializer to be a compatible value expression",
                ));
            }
            self.initializer_inner_impl(
                &value,
                ty,
                initializer,
                requires_constant,
                flexible,
                retained,
            )?;
            ty.clone()
        } else {
            self.initializer_inner_impl(ty, ty, initializer, requires_constant, flexible, retained)?
        };
        if let Some(id) = retained {
            self.code_builder().finish_initializer(id, &result)?;
        }
        Ok(result)
    }

    fn initializer_inner_impl(
        &mut self,
        ty: &Type,
        destination: &Type,
        initializer: InitializerRef<'_>,
        requires_constant: bool,
        mut flexible: Option<&mut FlexibleState>,
        retained: Option<InitializerId>,
    ) -> Result<Type, Error> {
        let offset = initializer.span.start;
        let resolved = self.unit.resolve(ty)?.clone();
        if self.unit.is_variable_length_array(ty)? {
            return Err(Error::new(
                offset,
                "variable-length arrays cannot have initializers",
            ));
        }
        if !matches!(resolved.kind, TypeKind::Array { length: None, .. })
            && !self.is_definite_object(ty, 0)?
        {
            return Err(Error::new(
                offset,
                "initializer requires a complete object type",
            ));
        }
        if requires_constant && self.unit.is_sizeless(ty)? {
            return Err(Error::new(
                offset,
                "objects with static or thread storage cannot have sizeless SVE type",
            ));
        }
        match initializer.node {
            InitializerView::Expression(expression) => {
                if matches!(resolved.kind, TypeKind::Array { .. }) {
                    let completed = self.string_initializer(ty, expression)?;
                    if let Some(id) = retained {
                        self.retained_initializer_string(id, &completed, expression)?;
                    }
                    return Ok(completed);
                }
                self.check_assignment(destination, expression)?;
                if requires_constant {
                    if matches!(resolved.kind, TypeKind::Vector { .. }) {
                        self.eval_vector(expression, true)?;
                    } else {
                        let kind = self.static_initializer(expression)?;
                        if kind == ConstantKind::Arithmetic
                            && matches!(
                                resolved.kind,
                                TypeKind::Integer(_)
                                    | TypeKind::Bool
                                    | TypeKind::Enum(_)
                                    | TypeKind::Float(_)
                                    | TypeKind::Complex(_)
                            )
                        {
                            let value = self.eval_initializer_arithmetic(expression)?;
                            self.convert_arithmetic(value, ty, offset)?;
                        }
                    }
                }
                if let Some(id) = retained {
                    self.retained_initializer_expression(id, destination, expression)?;
                }
                Ok(ty.clone())
            }
            InitializerView::List(items) => {
                if let Some(id) = retained {
                    let aggregate = matches!(
                        resolved.kind,
                        TypeKind::Array { .. } | TypeKind::Vector { .. } | TypeKind::Record(_)
                    );
                    let union_member = match resolved.kind {
                        TypeKind::Record(record)
                            if self.unit.records[record].kind == RecordKind::Union =>
                        {
                            self.first_subobject(ty)?.map(|index| index as usize)
                        }
                        _ => None,
                    };
                    self.code_builder().initializer_list(
                        id,
                        aggregate || items.is_empty(),
                        union_member,
                    );
                }
                if matches!(resolved.kind, TypeKind::Array { .. })
                    && let [item] = items
                    && item.node.designation.is_empty()
                    && let ast::Initializer::Expression(expression) = &item.node.initializer.node
                    && matches!(expression.node, ast::Expression::StringLiteral(_))
                    && self.character_array(&resolved)?
                {
                    let completed = self.string_initializer(ty, expression)?;
                    if let Some(id) = retained {
                        let child = self.code_builder().begin_initializer(
                            Origin::Written(&item.node.initializer),
                            ty,
                            requires_constant,
                        )?;
                        if let Some(child) = child {
                            self.retained_initializer_string(child, &completed, expression)?;
                            self.code_builder().finish_initializer(child, &completed)?;
                        }
                        self.code_builder()
                            .initializer_entry(id, item, RetainedPath::default())?;
                    }
                    return Ok(completed);
                }
                if !matches!(
                    resolved.kind,
                    TypeKind::Array { .. } | TypeKind::Vector { .. } | TypeKind::Record(_)
                ) {
                    if items.is_empty() {
                        return Ok(ty.clone());
                    }
                    let [item] = items else {
                        return Err(Error::new(offset, "scalar initializer requires one value"));
                    };
                    if !item.node.designation.is_empty() {
                        return Err(Error::new(
                            offset,
                            "scalar initializer cannot have a designator",
                        ));
                    }
                    let completed =
                        self.check_initializer(ty, &item.node.initializer, requires_constant)?;
                    if let Some(id) = retained {
                        self.code_builder()
                            .initializer_entry(id, item, RetainedPath::default())?;
                    }
                    return Ok(completed);
                }
                let mut cursor = self.first_subobject(ty)?.map(|index| vec![index]);
                let mut bound = 0;
                for item in items {
                    if matches!(resolved.kind, TypeKind::Vector { .. }) {
                        if !item.node.designation.is_empty() {
                            return Err(Error::new(
                                item.span.start,
                                "vector initializers cannot have designators",
                            ));
                        }
                        if matches!(item.node.initializer.node, ast::Initializer::List(_)) {
                            return Err(Error::new(
                                item.span.start,
                                "nested braces in vector lane initializers are unsupported",
                            ));
                        }
                    }
                    let mut retained_path = retained.map(|_| RetainedPath::default());
                    let mut path = if item.node.designation.is_empty() {
                        cursor.take().ok_or_else(|| {
                            Error::new(item.span.start, "excess elements in initializer")
                        })?
                    } else {
                        self.designated_subobject(
                            ty,
                            &item.node.designation,
                            retained_path.as_mut(),
                        )?
                    };
                    if item.node.designation.is_empty()
                        && let Some(output) = &mut retained_path
                    {
                        self.retained_initializer_path(
                            ty,
                            &path,
                            &mut output.steps,
                            item.span.start,
                        )?;
                    }
                    loop {
                        let target = self.subobject(ty, &path, item.span.start)?;
                        let member_depth = self.flexible_in_path(ty, &path, item.span.start)?;
                        if let Some(depth) = member_depth {
                            let top_level = depth == 1
                                && flexible
                                    .as_ref()
                                    .is_some_and(|state| state.member_index as u64 == path[0]);
                            let permitted =
                                top_level && flexible.as_ref().is_some_and(|state| state.permitted);
                            let empty = depth == path.len()
                                && self.empty_initializer(&item.node.initializer);
                            if !permitted
                                && (!empty || (!requires_constant && self.gnu_flexible_arrays()))
                            {
                                let message = if !requires_constant {
                                    "flexible array initialization requires static storage on this target"
                                } else if depth == 1
                                    && flexible.is_none()
                                    && requires_constant
                                    && self.gnu_flexible_arrays()
                                {
                                    "flexible-array allocation in a compound literal or nested object is unsupported"
                                } else {
                                    "nonempty flexible array initialization requires a top-level static object"
                                };
                                return Err(Error::new(item.span.start, message));
                            }
                            if !self.gnu_flexible_arrays()
                                && !item.node.designation.is_empty()
                                && depth < path.len()
                            {
                                return Err(Error::new(
                                    item.span.start,
                                    "designator into a flexible array member subobject is unsupported by the target",
                                ));
                            }
                        }
                        let aggregate = matches!(
                            self.unit.resolve(&target)?.kind,
                            TypeKind::Array { .. } | TypeKind::Vector { .. } | TypeKind::Record(_)
                        );
                        let whole = match &item.node.initializer.node {
                            ast::Initializer::List(_) => true,
                            ast::Initializer::Expression(expression) => {
                                !aggregate || self.initializes_whole_object(&target, expression)?
                            }
                        };
                        if member_depth == Some(path.len())
                            && !self.gnu_flexible_arrays()
                            && !item.node.designation.is_empty()
                            && !whole
                        {
                            return Err(Error::new(
                                item.span.start,
                                "flexible array designator requires a brace-enclosed or string initializer",
                            ));
                        }
                        if whole {
                            let completed = self.check_initializer(
                                &target,
                                &item.node.initializer,
                                requires_constant,
                            )?;
                            if member_depth == Some(1)
                                && path.len() == 1
                                && let Some(state) = flexible.as_deref_mut()
                            {
                                let TypeKind::Array {
                                    length: Some(length),
                                    ..
                                } = self.unit.resolve(&completed)?.kind
                                else {
                                    unreachable!("completed flexible array")
                                };
                                if length != 0
                                    || !self.gnu_flexible_arrays()
                                    || state.elements.is_none()
                                {
                                    state.elements = Some(length);
                                }
                            }
                            break;
                        }
                        let first = self.first_subobject(&target)?.ok_or_else(|| {
                            Error::new(item.span.start, "initializer has no object to initialize")
                        })?;
                        if path.len() >= 128 {
                            return Err(Error::new(
                                item.span.start,
                                "initializer subobject nesting limit exceeded",
                            ));
                        }
                        path.push(first);
                        if let Some(output) = &mut retained_path {
                            self.retained_initializer_path(
                                &target,
                                &[first],
                                &mut output.steps,
                                item.span.start,
                            )?;
                        }
                    }
                    if let (Some(id), Some(path)) = (retained, retained_path) {
                        self.code_builder().initializer_entry(id, item, path)?;
                    }
                    if let Some(state) = flexible.as_deref_mut()
                        && path.len() > 1
                        && path[0] == state.member_index as u64
                    {
                        let length = path[1].checked_add(1).ok_or_else(|| {
                            Error::new(item.span.start, "flexible array bound overflows")
                        })?;
                        state.elements = Some(state.elements.unwrap_or(0).max(length));
                    }
                    bound = bound.max(path[0].checked_add(1).ok_or_else(|| {
                        Error::new(item.span.start, "initializer array bound overflows")
                    })?);
                    cursor = self.next_subobject(ty, &path, item.span.start)?;
                }
                if let TypeKind::Array {
                    element,
                    length: None,
                } = resolved.kind
                {
                    Ok(Type {
                        kind: TypeKind::Array {
                            element,
                            length: Some(bound),
                        },
                        qualifiers: self.unit.qualifiers(ty)?,
                        alignment: self.unit.typedef_alignment_metadata(ty)?,
                    })
                } else {
                    Ok(ty.clone())
                }
            }
        }
    }

    fn character_array(&self, ty: &Type) -> Result<bool, Error> {
        let TypeKind::Array { element, .. } = &self.unit.resolve(ty)?.kind else {
            return Ok(false);
        };
        Ok(matches!(
            self.unit.resolve(element)?.kind,
            TypeKind::Integer(
                IntegerKind::Char
                    | IntegerKind::SignedChar
                    | IntegerKind::UnsignedChar
                    | IntegerKind::UnsignedShort
                    | IntegerKind::Int
                    | IntegerKind::Long
                    | IntegerKind::UnsignedInt
            ) | TypeKind::Enum(_)
        ))
    }

    fn string_initializer(
        &mut self,
        ty: &Type,
        expression: &Node<ast::Expression>,
    ) -> Result<Type, Error> {
        let offset = expression.span.start;
        if !matches!(expression.node, ast::Expression::StringLiteral(_))
            || !self.character_array(ty)?
        {
            return Err(Error::new(
                offset,
                "array expression initializer requires a compatible string literal",
            ));
        }
        let source = self.expression_type(expression)?;
        let TypeKind::Array {
            element: source_element,
            length: Some(string_length),
        } = source.kind
        else {
            unreachable!("string expression has an array type")
        };
        let mut result = self.unit.resolve(ty)?.clone();
        result.qualifiers = self.unit.qualifiers(ty)?;
        let TypeKind::Array { element, length } = &mut result.kind else {
            unreachable!()
        };
        let source_kind = &self.unit.resolve(&source_element)?.kind;
        let destination_kind = &self.unit.resolve(element)?.kind;
        let compatible = if *source_kind == TypeKind::Integer(IntegerKind::Char) {
            matches!(
                destination_kind,
                TypeKind::Integer(
                    IntegerKind::Char | IntegerKind::SignedChar | IntegerKind::UnsignedChar
                )
            )
        } else if matches!(destination_kind, TypeKind::Enum(_)) {
            self.integer_type(&source_element, offset)? == self.integer_type(element, offset)?
        } else {
            source_kind == destination_kind
        };
        if !compatible {
            return Err(Error::new(
                offset,
                "string encoding is incompatible with the array element type",
            ));
        }
        match length {
            Some(bound) if *bound < string_length - 1 => {
                return Err(Error::new(
                    offset,
                    "string literal is too long for the array",
                ));
            }
            None => *length = Some(string_length),
            _ => {}
        }
        Ok(result)
    }

    fn initializes_whole_object(
        &mut self,
        ty: &Type,
        expression: &Node<ast::Expression>,
    ) -> Result<bool, Error> {
        if matches!(expression.node, ast::Expression::StringLiteral(_))
            && self.character_array(ty)?
        {
            return Ok(true);
        }
        if matches!(
            self.unit.resolve(ty)?.kind,
            TypeKind::Record(_) | TypeKind::Vector { .. }
        ) {
            let source = self.expression_type(expression)?;
            let source = self.value_type(&source)?;
            let mut destination = self.unit.resolve(ty)?.clone();
            destination.qualifiers = Default::default();
            return self.compatible(&destination, &source);
        }
        Ok(false)
    }

    fn first_subobject(&self, ty: &Type) -> Result<Option<u64>, Error> {
        Ok(match &self.unit.resolve(ty)?.kind {
            TypeKind::Array {
                length: Some(0), ..
            } => None,
            TypeKind::Array { .. } | TypeKind::Vector { .. } => Some(0),
            TypeKind::Record(id) => self
                .unit
                .records
                .get(*id)
                .and_then(|record| record.fields.as_ref())
                .and_then(|fields| {
                    fields
                        .iter()
                        .position(|field| field.name.is_some() || field.bit_width.is_none())
                })
                .map(|index| index as u64),
            _ => None,
        })
    }

    pub(crate) fn subobject(
        &self,
        root: &Type,
        path: &[u64],
        offset: usize,
    ) -> Result<Type, Error> {
        let mut ty = root.clone();
        for index in path {
            ty = match &self.unit.resolve(&ty)?.kind {
                TypeKind::Array { element, length }
                    if length.is_none_or(|length| *index < length) =>
                {
                    (**element).clone()
                }
                TypeKind::Vector { element, lanes, .. } if *index < *lanes => (**element).clone(),
                TypeKind::Record(id) => self
                    .unit
                    .records
                    .get(*id)
                    .and_then(|record| record.fields.as_ref())
                    .and_then(|fields| {
                        usize::try_from(*index)
                            .ok()
                            .and_then(|index| fields.get(index))
                    })
                    .map(|field| field.ty.clone())
                    .ok_or_else(|| Error::new(offset, "invalid record initializer designator"))?,
                _ => {
                    return Err(Error::new(
                        offset,
                        "initializer designator is outside the object",
                    ));
                }
            };
        }
        Ok(ty)
    }

    fn next_subobject(
        &self,
        root: &Type,
        path: &[u64],
        offset: usize,
    ) -> Result<Option<Vec<u64>>, Error> {
        let mut next = path.to_vec();
        while let Some(index) = next.pop() {
            let parent = self.subobject(root, &next, offset)?;
            let sibling = match &self.unit.resolve(&parent)?.kind {
                TypeKind::Vector { lanes, .. } => {
                    index.checked_add(1).filter(|index| *index < *lanes)
                }
                TypeKind::Array { length, .. } => index
                    .checked_add(1)
                    .filter(|index| length.is_none_or(|length| *index < length)),
                TypeKind::Record(id) => {
                    let record = &self.unit.records[*id];
                    if record.kind == RecordKind::Union {
                        None
                    } else {
                        record.fields.as_ref().and_then(|fields| {
                            fields
                                .iter()
                                .enumerate()
                                .skip(index as usize + 1)
                                .find(|(_, field)| {
                                    field.name.is_some() || field.bit_width.is_none()
                                })
                                .map(|(index, _)| index as u64)
                        })
                    }
                }
                _ => None,
            };
            if let Some(index) = sibling {
                next.push(index);
                return Ok(Some(next));
            }
        }
        Ok(None)
    }

    fn designated_subobject(
        &mut self,
        root: &Type,
        designators: &[Node<ast::Designator>],
        mut retained: Option<&mut RetainedPath>,
    ) -> Result<Vec<u64>, Error> {
        let mut path = Vec::new();
        for designator in designators {
            let offset = designator.span.start;
            let ty = self.subobject(root, &path, offset)?;
            if let Some(output) = retained.as_deref_mut() {
                self.retained_initializer_designator(designator, output)?;
            }
            match &designator.node {
                ast::Designator::Index(expression) => {
                    if !matches!(self.unit.resolve(&ty)?.kind, TypeKind::Array { .. }) {
                        return Err(Error::new(offset, "array designator requires an array"));
                    }
                    let index = self.eval(expression)?.as_u64()?;
                    path.push(index);
                    if let Some(output) = retained.as_deref_mut() {
                        output.steps.push(Subobject::Index {
                            index,
                            expression: Some(self.retained_expression_id(expression)?),
                        });
                    }
                }
                ast::Designator::Range(range) => {
                    if !matches!(self.unit.resolve(&ty)?.kind, TypeKind::Array { .. }) {
                        return Err(Error::new(
                            offset,
                            "array range designator requires an array",
                        ));
                    }
                    let from = self.eval(&range.node.from)?.as_u64()?;
                    let to = self.eval(&range.node.to)?.as_u64()?;
                    if from > to {
                        return Err(Error::new(offset, "array designator range is reversed"));
                    }
                    // Both endpoints must be in bounds. Check the initializer once
                    // and advance past the final element without expanding the range.
                    let mut first = path.clone();
                    first.push(from);
                    self.subobject(root, &first, offset)?;
                    path.push(to);
                    if let Some(output) = retained.as_deref_mut() {
                        output.steps.push(Subobject::Range {
                            start: from,
                            end: to,
                            from: self.retained_expression_id(&range.node.from)?,
                            to: self.retained_expression_id(&range.node.to)?,
                        });
                    }
                }
                ast::Designator::Member(member) => {
                    let member_path = self
                        .member_designator(&ty, &member.node.name, 0)?
                        .ok_or_else(|| {
                            Error::new(
                                offset,
                                format!("unknown initializer member `{}`", member.node.name),
                            )
                        })?;
                    if let Some(output) = retained.as_deref_mut() {
                        self.retain_member_reference(
                            &ty,
                            &member_path
                                .iter()
                                .map(|index| *index as usize)
                                .collect::<Vec<_>>(),
                            member,
                        )?;
                        self.retained_initializer_path(
                            &ty,
                            &member_path,
                            &mut output.steps,
                            offset,
                        )?;
                    }
                    path.extend(member_path);
                }
            }
            if path.len() >= 128 {
                return Err(Error::new(
                    offset,
                    "initializer designator nesting limit exceeded",
                ));
            }
            self.subobject(root, &path, offset)?;
        }
        Ok(path)
    }

    pub(crate) fn member_designator(
        &self,
        ty: &Type,
        name: &str,
        depth: usize,
    ) -> Result<Option<Vec<u64>>, Error> {
        if depth >= 128 {
            return Err(Error::new(0, "anonymous member nesting limit exceeded"));
        }
        let TypeKind::Record(id) = self.unit.resolve(ty)?.kind else {
            return Ok(None);
        };
        let Some(fields) = self
            .unit
            .records
            .get(id)
            .and_then(|record| record.fields.as_ref())
        else {
            return Ok(None);
        };
        for (index, field) in fields.iter().enumerate() {
            if field.name.as_deref() == Some(name) {
                return Ok(Some(vec![index as u64]));
            }
            if field.name.is_none()
                && field.bit_width.is_none()
                && let Some(mut nested) = self.member_designator(&field.ty, name, depth + 1)?
            {
                nested.insert(0, index as u64);
                return Ok(Some(nested));
            }
        }
        Ok(None)
    }

    /// Validates a scalar operand of a static vector expression before folding it.
    pub(crate) fn check_static_arithmetic(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<(), Error> {
        if self.static_initializer(expression)? == ConstantKind::Arithmetic {
            Ok(())
        } else {
            Err(Error::new(
                expression.span.start,
                "static vector lane requires an arithmetic constant",
            ))
        }
    }

    fn static_initializer(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<ConstantKind, Error> {
        self.enter_expression(expression.span.start)?;
        let result =
            self.with_const_object_reads(|analyzer| analyzer.static_initializer_inner(expression));
        self.leave_expression();
        result
    }

    fn static_initializer_inner(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<ConstantKind, Error> {
        let offset = expression.span.start;
        let invalid = || {
            Error::new(
                offset,
                "static storage initializer is not a constant expression",
            )
        };
        match &expression.node {
            ast::Expression::Statement(_) => Err(Error::new(
                offset,
                "constant evaluation of statement expressions is unsupported",
            )),
            ast::Expression::Constant(_) => Ok(ConstantKind::Arithmetic),
            ast::Expression::Call(call)
                if self
                    .builtin_name(call)
                    .and_then(|name| self.object_size_signature(name))
                    .is_some() =>
            {
                self.eval_object_size(call)?;
                Ok(ConstantKind::Arithmetic)
            }

            ast::Expression::Call(call)
                if self.builtin_name(call) == Some("__builtin_constant_p") =>
            {
                // A required constant-expression site never executes Clang's
                // scalar code-generation fallback. Unknown operands fold to 0.
                self.eval_constant_query(call)?;
                Ok(ConstantKind::Arithmetic)
            }
            ast::Expression::Call(call)
                if self
                    .builtin_name(call)
                    .and_then(|name| self.infinity_builtin_kind(name))
                    .is_some()
                    || self
                        .builtin_name(call)
                        .and_then(|name| self.nan_builtin(name))
                        .is_some()
                    || self.builtin_name(call) == Some("__builtin_complex")
                    || self
                        .builtin_name(call)
                        .is_some_and(|name| self.complex_unary_builtin(name).is_some()) =>
            {
                self.eval_initializer_arithmetic(expression)?;
                Ok(ConstantKind::Arithmetic)
            }
            ast::Expression::SizeOfTy(_)
            | ast::Expression::SizeOfVal(_)
            | ast::Expression::AlignOf(_)
            | ast::Expression::OffsetOf(_) => {
                self.eval(expression)?;
                Ok(ConstantKind::Arithmetic)
            }
            ast::Expression::CompoundLiteral(literal) => {
                let ty = self.type_name(&literal.node.type_name.node)?;
                let ty = self.check_initializer_list(
                    &ty,
                    &literal.node.initializer_list,
                    expression,
                    literal.span,
                    true,
                )?;
                Ok(
                    if matches!(self.unit.resolve(&ty)?.kind, TypeKind::Array { .. }) {
                        if self.in_function_body() {
                            return Err(invalid());
                        }
                        ConstantKind::Address
                    } else {
                        ConstantKind::Arithmetic
                    },
                )
            }
            ast::Expression::StringLiteral(_) => Ok(ConstantKind::Address),
            ast::Expression::Member(_) => self.static_designator(expression),
            ast::Expression::Identifier(identifier) => {
                if self.unit.constants.contains_key(&identifier.node.name)
                    || self.const_object_value(&identifier.node.name).is_some()
                {
                    return Ok(ConstantKind::Arithmetic);
                }
                let ty = self.expression_type(expression)?;
                if matches!(
                    self.unit.resolve(&ty)?.kind,
                    TypeKind::Array { .. } | TypeKind::Function(_)
                ) && (self.object_has_static_storage(&identifier.node.name)
                    && !self.dll_imported_object(&identifier.node.name)
                    || self
                        .builtin_function_reference(&identifier.node.name)?
                        .is_some())
                {
                    Ok(ConstantKind::Address)
                } else {
                    Err(invalid())
                }
            }
            ast::Expression::UnaryOperator(unary) => match unary.node.operator.node {
                ast::UnaryOperator::Address => {
                    self.static_lvalue(&unary.node.operand)?;
                    Ok(ConstantKind::Address)
                }
                ast::UnaryOperator::Indirection => self.static_designator(expression),
                ast::UnaryOperator::Plus
                | ast::UnaryOperator::Minus
                | ast::UnaryOperator::Complement
                | ast::UnaryOperator::Negate
                | ast::UnaryOperator::Real
                | ast::UnaryOperator::Imaginary => {
                    if self.static_initializer(&unary.node.operand)? == ConstantKind::Arithmetic {
                        Ok(ConstantKind::Arithmetic)
                    } else {
                        Err(invalid())
                    }
                }
                _ => Err(invalid()),
            },
            ast::Expression::Cast(cast) => {
                let kind = self.static_initializer(&cast.node.expression)?;
                let ty = self.type_name(&cast.node.type_name.node)?;
                if matches!(self.unit.resolve(&ty)?.kind, TypeKind::Pointer(_)) {
                    if kind == ConstantKind::Arithmetic {
                        self.eval(&cast.node.expression)?;
                    }
                    Ok(ConstantKind::Address)
                } else if kind == ConstantKind::Arithmetic {
                    Ok(kind)
                } else {
                    Err(invalid())
                }
            }
            ast::Expression::BinaryOperator(binary) => {
                use ast::BinaryOperator as Op;
                if binary.node.operator.node == Op::Index {
                    return self.static_designator(expression);
                }
                if !matches!(
                    binary.node.operator.node,
                    Op::Multiply
                        | Op::Divide
                        | Op::Modulo
                        | Op::Plus
                        | Op::Minus
                        | Op::ShiftLeft
                        | Op::ShiftRight
                        | Op::Less
                        | Op::LessOrEqual
                        | Op::Greater
                        | Op::GreaterOrEqual
                        | Op::Equals
                        | Op::NotEquals
                        | Op::BitwiseAnd
                        | Op::BitwiseOr
                        | Op::BitwiseXor
                        | Op::LogicalAnd
                        | Op::LogicalOr
                ) {
                    return Err(invalid());
                }
                let left = self.static_initializer(&binary.node.lhs)?;
                if let Ok(value) = self.eval_initializer_arithmetic(&binary.node.lhs)
                    && ((binary.node.operator.node == Op::LogicalAnd && !value.truth())
                        || (binary.node.operator.node == Op::LogicalOr && value.truth()))
                {
                    return Ok(ConstantKind::Arithmetic);
                }
                let right = self.static_initializer(&binary.node.rhs)?;
                if left == ConstantKind::Arithmetic && right == ConstantKind::Arithmetic {
                    self.eval_initializer_arithmetic(expression)?;
                    Ok(ConstantKind::Arithmetic)
                } else if left == ConstantKind::Address
                    && right == ConstantKind::Arithmetic
                    && matches!(binary.node.operator.node, Op::Plus | Op::Minus)
                {
                    self.eval(&binary.node.rhs)?;
                    Ok(ConstantKind::Address)
                } else if left == ConstantKind::Arithmetic
                    && right == ConstantKind::Address
                    && binary.node.operator.node == Op::Plus
                {
                    self.eval(&binary.node.lhs)?;
                    Ok(ConstantKind::Address)
                } else {
                    Err(invalid())
                }
            }
            ast::Expression::Conditional(conditional) => {
                let kind = self.static_initializer(&conditional.node.condition)?;
                match self.static_conditional_branch(&conditional.node, offset)? {
                    ConstantBranch::Reused(_) => Ok(kind),
                    ConstantBranch::Selected(selected) => self.static_initializer(selected),
                }
            }
            ast::Expression::TypesCompatible(query) => {
                self.eval_types_compatible(query)?;
                Ok(ConstantKind::Arithmetic)
            }
            ast::Expression::Choose(selection) => {
                let selected = self.choose_expression(selection)?;
                self.static_initializer(selected)
            }
            ast::Expression::GenericSelection(selection) => {
                let selected = self.generic_expression(selection)?;
                self.static_initializer(selected)
            }
            _ => Err(invalid()),
        }
    }

    /// Selects a static conditional using declaration-time symbol binding. A weak
    /// address can be null, even when a later declaration provides a definition.
    fn static_conditional_branch<'a>(
        &mut self,
        conditional: &'a ast::ConditionalExpression,
        offset: usize,
    ) -> Result<ConstantBranch<'a>, Error> {
        let condition = &conditional.condition;
        let condition_ty = self.value_expression_type(condition)?;
        if !matches!(self.unit.resolve(&condition_ty)?.kind, TypeKind::Pointer(_)) {
            return Ok(if self.eval_initializer_arithmetic(condition)?.truth() {
                match &conditional.then_expression {
                    Some(value) => ConstantBranch::Selected(value),
                    None => ConstantBranch::Reused(None),
                }
            } else {
                ConstantBranch::Selected(&conditional.else_expression)
            });
        }
        let address = self.static_pointer(condition, false)?;
        if let Some(truth) = address.and_then(PointerConstant::truth) {
            return Ok(if truth {
                match &conditional.then_expression {
                    Some(value) => ConstantBranch::Selected(value),
                    None => ConstantBranch::Reused(address),
                }
            } else {
                ConstantBranch::Selected(&conditional.else_expression)
            });
        }
        // GCC preserves the address relocation for `p ? p : 0`, including
        // omitted-middle syntax, without assuming a weak p is nonzero. Clang
        // does not admit this as a static initializer. Compare symbolic identity
        // and byte offsets for an explicit middle operand; unrelated addresses
        // cannot justify this fold.
        if self.gnu_sync_profile()
            && let Some(address) = address
            && (self
                .eval(&conditional.else_expression)
                .is_ok_and(|v| v.value == 0)
                || self
                    .static_pointer(&conditional.else_expression, false)?
                    .is_some_and(|value| value.base.is_none() && value.offset == 0))
            && (conditional.then_expression.is_none()
                || self.static_pointer(conditional.nonzero_expression(), false)? == Some(address))
        {
            return Ok(match &conditional.then_expression {
                Some(value) => ConstantBranch::Selected(value),
                None => ConstantBranch::Reused(Some(address)),
            });
        }
        Err(Error::new(
            offset,
            "static storage initializer has no constant pointer condition",
        ))
    }

    /// Resolves a checked address constant without reading an object. The caller
    /// separately checks static-storage eligibility; this bounded walk preserves
    /// symbol identity, weak binding, and target-width absolute pointer bits.
    fn static_pointer<'a>(
        &mut self,
        expression: &'a Node<ast::Expression>,
        location: bool,
    ) -> Result<Option<PointerConstant<'a>>, Error> {
        self.enter_expression(expression.span.start)?;
        let result = self.static_pointer_inner(expression, location);
        self.leave_expression();
        result
    }

    fn static_pointer_inner<'a>(
        &mut self,
        expression: &'a Node<ast::Expression>,
        location: bool,
    ) -> Result<Option<PointerConstant<'a>>, Error> {
        use ast::{BinaryOperator as Binary, Expression as E, UnaryOperator as Unary};
        let offset = expression.span.start;
        let designator = matches!(&expression.node, E::Member(_) | E::CompoundLiteral(_))
            || matches!(&expression.node, E::UnaryOperator(unary) if unary.node.operator.node == Unary::Indirection)
            || matches!(&expression.node, E::BinaryOperator(binary) if binary.node.operator.node == Binary::Index);
        if !location && designator {
            let ty = self.expression_type(expression)?;
            if !matches!(
                self.unit.resolve(&ty)?.kind,
                TypeKind::Array { .. } | TypeKind::Function(_)
            ) {
                return Ok(None);
            }
        }
        let mask = u128::MAX >> (128 - self.unit.target.pointer_width());
        let result = match &expression.node {
            E::Identifier(identifier) => {
                let name = identifier.node.name.as_str();
                let ty = self.expression_type(expression)?;
                if !location
                    && !matches!(
                        self.unit.resolve(&ty)?.kind,
                        TypeKind::Array { .. } | TypeKind::Function(_)
                    )
                {
                    return Ok(None);
                }
                if !self.object_has_static_storage(name)
                    && self.builtin_function_reference(name)?.is_none()
                {
                    return Ok(None);
                }
                let linked = self
                    .lexical_scopes
                    .iter()
                    .rev()
                    .find(|scope| scope.names.contains_key(name))
                    .is_none_or(|scope| scope.linked.contains(name));
                PointerConstant {
                    base: Some(PointerBase::Symbol(name)),
                    offset: 0,
                    nullable: linked && self.weak_symbols.contains_key(name),
                }
            }
            E::StringLiteral(_) | E::CompoundLiteral(_) => PointerConstant {
                base: Some(PointerBase::Anonymous(
                    expression.span.start,
                    expression.span.end,
                )),
                offset: 0,
                nullable: false,
            },
            E::UnaryOperator(unary) if unary.node.operator.node == Unary::Address && !location => {
                return self.static_pointer(&unary.node.operand, true);
            }
            E::UnaryOperator(unary) if unary.node.operator.node == Unary::Indirection => {
                return self.static_pointer(&unary.node.operand, false);
            }
            E::Cast(cast) if !location => {
                let ty = self.type_name(&cast.node.type_name.node)?;
                if !matches!(self.unit.resolve(&ty)?.kind, TypeKind::Pointer(_)) {
                    return Ok(None);
                }
                if let Ok(value) = self.eval(&cast.node.expression) {
                    PointerConstant {
                        base: None,
                        offset: if value.signed {
                            value.signed_value() as u128
                        } else {
                            value.value
                        } & mask,
                        nullable: false,
                    }
                } else {
                    return self.static_pointer(&cast.node.expression, false);
                }
            }
            E::Member(member) => {
                let direct = member.node.operator.node == ast::MemberOperator::Direct;
                let Some(mut address) = self.static_pointer(&member.node.expression, direct)?
                else {
                    return Ok(None);
                };
                let mut ty = self.expression_type(&member.node.expression)?;
                if !direct {
                    let TypeKind::Pointer(pointee) = &self.unit.resolve(&ty)?.kind else {
                        return Ok(None);
                    };
                    ty = (**pointee).clone();
                }
                let (bytes, _) =
                    self.field_offset(&ty, &member.node.identifier.node.name, offset)?;
                address.offset = address.offset.wrapping_add(u128::from(bytes)) & mask;
                address
            }
            E::BinaryOperator(binary)
                if matches!(
                    binary.node.operator.node,
                    Binary::Plus | Binary::Minus | Binary::Index
                ) =>
            {
                let left_ty = self.value_expression_type(&binary.node.lhs)?;
                let (pointer, integer, pointer_ty) =
                    if matches!(self.unit.resolve(&left_ty)?.kind, TypeKind::Pointer(_)) {
                        (&binary.node.lhs, &binary.node.rhs, left_ty)
                    } else if binary.node.operator.node != Binary::Minus {
                        let ty = self.value_expression_type(&binary.node.rhs)?;
                        (&binary.node.rhs, &binary.node.lhs, ty)
                    } else {
                        return Ok(None);
                    };
                let Some(mut address) = self.static_pointer(pointer, false)? else {
                    return Ok(None);
                };
                let TypeKind::Pointer(pointee) = &self.unit.resolve(&pointer_ty)?.kind else {
                    return Ok(None);
                };
                let stride = match self.unit.resolve(pointee)?.kind {
                    TypeKind::Void | TypeKind::Function(_) => 1,
                    _ => self.unit.layout(pointee)?.size_bytes(),
                };
                let value = self.eval(integer)?;
                let index = if value.signed {
                    value.signed_value() as u128
                } else {
                    value.value
                };
                let bytes = index.wrapping_mul(u128::from(stride));
                address.offset = if binary.node.operator.node == Binary::Minus {
                    address.offset.wrapping_sub(bytes)
                } else {
                    address.offset.wrapping_add(bytes)
                } & mask;
                address
            }
            E::Conditional(conditional) if !location => {
                let selected = match self.static_conditional_branch(&conditional.node, offset)? {
                    ConstantBranch::Reused(address) => return Ok(address),
                    ConstantBranch::Selected(selected) => selected,
                };
                // The conditional's pointer result converts a selected integer
                // null pointer constant before an enclosing condition uses it.
                if self.eval(selected).is_ok_and(|value| value.value == 0) {
                    PointerConstant {
                        base: None,
                        offset: 0,
                        nullable: false,
                    }
                } else {
                    return self.static_pointer(selected, false);
                }
            }
            E::Choose(selection) => {
                let selected = self.choose_expression(selection)?;
                return self.static_pointer(selected, location);
            }
            E::GenericSelection(selection) => {
                let selected = self.generic_expression(selection)?;
                return self.static_pointer(selected, location);
            }
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    /// Array and function designators form addresses without reading their objects.
    /// Scalar subobjects still require a load and cannot initialize static storage.
    fn static_designator(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<ConstantKind, Error> {
        let ty = self.expression_type(expression)?;
        if matches!(
            self.unit.resolve(&ty)?.kind,
            TypeKind::Array { .. } | TypeKind::Function(_)
        ) {
            self.static_lvalue(expression)?;
            Ok(ConstantKind::Address)
        } else {
            Err(Error::new(
                expression.span.start,
                "static storage initializer is not a constant expression",
            ))
        }
    }

    fn static_lvalue(&mut self, expression: &Node<ast::Expression>) -> Result<(), Error> {
        self.enter_expression(expression.span.start)?;
        let result = self.static_lvalue_inner(expression);
        self.leave_expression();
        result
    }

    fn static_lvalue_inner(&mut self, expression: &Node<ast::Expression>) -> Result<(), Error> {
        let offset = expression.span.start;
        let invalid = || {
            Error::new(
                offset,
                "address initializer requires an object with static storage",
            )
        };
        match &expression.node {
            ast::Expression::GenericSelection(selection) => {
                let selected = self.generic_expression(selection)?;
                self.static_lvalue(selected)
            }
            ast::Expression::Choose(selection) => {
                let selected = self.choose_expression(selection)?;
                self.static_lvalue(selected)
            }

            ast::Expression::Identifier(identifier) => {
                if self.object_has_static_storage(&identifier.node.name)
                    && !self.dll_imported_object(&identifier.node.name)
                    || self
                        .builtin_function_reference(&identifier.node.name)?
                        .is_some()
                {
                    Ok(())
                } else {
                    Err(invalid())
                }
            }
            ast::Expression::CompoundLiteral(_) => {
                if self.in_function_body() {
                    return Err(invalid());
                }
                self.static_initializer(expression)?;
                Ok(())
            }
            ast::Expression::StringLiteral(_) => Ok(()),
            ast::Expression::Member(member) => {
                if member.node.operator.node == ast::MemberOperator::Direct {
                    self.static_lvalue(&member.node.expression)
                } else if self.static_initializer(&member.node.expression)? == ConstantKind::Address
                {
                    Ok(())
                } else {
                    Err(invalid())
                }
            }
            ast::Expression::UnaryOperator(unary)
                if matches!(
                    unary.node.operator.node,
                    ast::UnaryOperator::Real | ast::UnaryOperator::Imaginary
                ) =>
            {
                let ty = self.expression_type(&unary.node.operand)?;
                if self.gnu_sync_profile()
                    && matches!(self.unit.resolve(&ty)?.kind, TypeKind::Complex(_))
                {
                    return Err(Error::new(
                        offset,
                        "GNU complex component addresses are not static initializer constants",
                    ));
                }
                self.static_lvalue(&unary.node.operand)
            }
            ast::Expression::UnaryOperator(unary)
                if unary.node.operator.node == ast::UnaryOperator::Indirection =>
            {
                if self.static_initializer(&unary.node.operand)? == ConstantKind::Address {
                    Ok(())
                } else {
                    Err(invalid())
                }
            }
            ast::Expression::BinaryOperator(binary)
                if binary.node.operator.node == ast::BinaryOperator::Index =>
            {
                let base = self.expression_type(&binary.node.lhs)?;
                if matches!(self.unit.resolve(&base)?.kind, TypeKind::Vector { .. }) {
                    self.static_lvalue(&binary.node.lhs)?;
                    self.eval(&binary.node.rhs)?;
                    return Ok(());
                }
                let left = self.static_initializer(&binary.node.lhs)?;
                let right = self.static_initializer(&binary.node.rhs)?;
                if left == ConstantKind::Address && right == ConstantKind::Arithmetic {
                    self.eval(&binary.node.rhs)?;
                    Ok(())
                } else if left == ConstantKind::Arithmetic && right == ConstantKind::Address {
                    self.eval(&binary.node.lhs)?;
                    Ok(())
                } else {
                    Err(invalid())
                }
            }
            _ => Err(invalid()),
        }
    }
}

impl Analyzer {
    /// Retain destination-converted arithmetic values after ordinary C checking.
    /// Address constants and enum objects have no scalar literal projection.
    pub(crate) fn scalar_object_value(
        &mut self,
        ty: &Type,
        initializer: &Node<ast::Initializer>,
    ) -> Result<Option<crate::ArithmeticConstant>, Error> {
        self.scalar_initializer_value(ty, initializer, false)?
            .map(|value| value.into_constant(self.unit.target, initializer.span.start))
            .transpose()
    }

    /// Evaluate one scalar initializer with the same destination conversion used
    /// by its declaration. Enum objects can seed Clang initializer reads without
    /// changing the separate binding projection's enum policy.
    pub(crate) fn scalar_initializer_value(
        &mut self,
        ty: &Type,
        initializer: &Node<ast::Initializer>,
        include_enums: bool,
    ) -> Result<Option<crate::floating::ArithmeticValue>, Error> {
        let destination = self.unit.atomic_value(ty)?.unwrap_or(ty).clone();
        if !matches!(
            self.unit.resolve(&destination)?.kind,
            TypeKind::Integer(_) | TypeKind::Bool | TypeKind::Float(_)
        ) && !(include_enums
            && matches!(self.unit.resolve(&destination)?.kind, TypeKind::Enum(_)))
        {
            return Ok(None);
        }
        let mut initializer = initializer;
        for _ in 0..128 {
            match &initializer.node {
                ast::Initializer::Expression(expression) => {
                    if self.static_initializer(expression)? != ConstantKind::Arithmetic {
                        return Ok(None);
                    }
                    let value = self.eval_initializer_arithmetic(expression)?;
                    let value =
                        self.convert_arithmetic(value, &destination, initializer.span.start)?;
                    return Ok(Some(value));
                }
                ast::Initializer::List(items) => {
                    if items.is_empty() {
                        let zero = crate::floating::ArithmeticValue::Integer(crate::IntegerValue {
                            value: 0,
                            bits: 32,
                            signed: true,
                            rank: 3,
                        });
                        let value =
                            self.convert_arithmetic(zero, &destination, initializer.span.start)?;
                        return Ok(Some(value));
                    }
                    let [item] = items.as_slice() else {
                        return Ok(None);
                    };
                    initializer = &item.node.initializer;
                }
            }
        }
        Err(Error::new(
            initializer.span.start,
            "object-value initializer nesting exceeds the 128-level limit",
        ))
    }
}
