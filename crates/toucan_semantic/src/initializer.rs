use lang_c::{
    ast,
    span::{Node, Span},
};

use crate::analyze::Analyzer;
use crate::{
    DeclarationKind, Error, FlexibleArrayStorage, IntegerKind, RecordKind, Type, TypeKind,
};

#[derive(Clone, Copy, PartialEq)]
enum ConstantKind {
    Arithmetic,
    Address,
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
    node: InitializerView<'a>,
    span: Span,
}

impl<'a> From<&'a Node<ast::Initializer>> for InitializerRef<'a> {
    fn from(initializer: &'a Node<ast::Initializer>) -> Self {
        Self {
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
    ) -> Result<(), Error> {
        if self.unit.declarations[index].kind != DeclarationKind::Variable {
            return Err(Error::new(
                initializer.span.start,
                "only an object can have an initializer",
            ));
        }
        let (completed, storage) = self.check_object_initializer(original, initializer, true)?;
        self.unit.declarations[index].flexible_array_storage = storage;
        let previous = &self.unit.declarations[index].ty;
        if !self.compatible(previous, &completed)? {
            return Err(Error::new(
                initializer.span.start,
                "initializer array bound conflicts with the previous declaration",
            ));
        }
        let ty = self.composite_type(previous, &completed, 0)?;
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
        static_storage: bool,
    ) -> Result<Type, Error> {
        self.enter_expression(initializer.span.start)?;
        let result = self.initializer_inner(ty, initializer.into(), static_storage, None);
        self.leave_expression();
        result
    }

    /// Compound literals borrow their written list, preserving occurrence identity
    /// and avoiding a fresh copy each time their type is queried.
    pub(crate) fn check_initializer_list(
        &mut self,
        ty: &Type,
        items: &[Node<ast::InitializerListItem>],
        span: Span,
        static_storage: bool,
    ) -> Result<Type, Error> {
        self.enter_expression(span.start)?;
        let result = self.initializer_inner(
            ty,
            InitializerRef {
                node: InitializerView::List(items),
                span,
            },
            static_storage,
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
        static_storage: bool,
    ) -> Result<(Type, Option<FlexibleArrayStorage>), Error> {
        let TypeKind::Record(id) = self.unit.resolve(ty)?.kind else {
            return self
                .check_initializer(ty, initializer, static_storage)
                .map(|ty| (ty, None));
        };
        let Some(fields) = self.unit.records[id].fields.as_ref() else {
            return self
                .check_initializer(ty, initializer, static_storage)
                .map(|ty| (ty, None));
        };
        let Some((member_index, field)) = fields.iter().enumerate().next_back() else {
            return self
                .check_initializer(ty, initializer, static_storage)
                .map(|ty| (ty, None));
        };
        let TypeKind::Array {
            element,
            length: None,
        } = &self.unit.resolve(&field.ty)?.kind
        else {
            return self
                .check_initializer(ty, initializer, static_storage)
                .map(|ty| (ty, None));
        };
        let element = (**element).clone();
        let mut flexible = FlexibleState {
            member_index,
            elements: None,
            permitted: static_storage,
        };
        self.enter_expression(initializer.span.start)?;
        let result =
            self.initializer_inner(ty, initializer.into(), static_storage, Some(&mut flexible));
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
        matches!(
            self.unit.target,
            toucan_target::Target::X86_64UnknownLinuxGnu
                | toucan_target::Target::Aarch64UnknownLinuxGnu
        )
    }

    fn empty_initializer(&self, initializer: &Node<ast::Initializer>) -> bool {
        matches!(&initializer.node, ast::Initializer::List(items) if items.is_empty() || (items.len() == 1 && self.empty_initializers.contains(&items[0].span.start)))
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
        static_storage: bool,
        mut flexible: Option<&mut FlexibleState>,
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
            && !self.is_complete_object(ty, 0)?
        {
            return Err(Error::new(
                offset,
                "initializer requires a complete object type",
            ));
        }
        match initializer.node {
            InitializerView::Expression(expression) => {
                if matches!(resolved.kind, TypeKind::Array { .. }) {
                    return self.string_initializer(ty, expression);
                }
                self.check_assignment(ty, expression)?;
                if static_storage {
                    let kind = self.static_initializer(expression)?;
                    if kind == ConstantKind::Arithmetic
                        && matches!(
                            resolved.kind,
                            TypeKind::Integer(_)
                                | TypeKind::Bool
                                | TypeKind::Enum(_)
                                | TypeKind::Float(_)
                        )
                    {
                        let value = self.eval_arithmetic(expression)?;
                        self.convert_arithmetic(value, ty, offset)?;
                    }
                }
                Ok(ty.clone())
            }
            InitializerView::List(items) => {
                let items =
                    if items.len() == 1 && self.empty_initializers.contains(&items[0].span.start) {
                        &[][..]
                    } else {
                        items
                    };
                if matches!(resolved.kind, TypeKind::Array { .. })
                    && let [item] = items
                    && item.node.designation.is_empty()
                    && let ast::Initializer::Expression(expression) = &item.node.initializer.node
                    && matches!(expression.node, ast::Expression::StringLiteral(_))
                    && self.character_array(&resolved)?
                {
                    return self.string_initializer(ty, expression);
                }
                if !matches!(resolved.kind, TypeKind::Array { .. } | TypeKind::Record(_)) {
                    let [item] = items else {
                        return Err(Error::new(offset, "scalar initializer requires one value"));
                    };
                    if !item.node.designation.is_empty() {
                        return Err(Error::new(
                            offset,
                            "scalar initializer cannot have a designator",
                        ));
                    }
                    return self.check_initializer(ty, &item.node.initializer, static_storage);
                }
                let mut cursor = self.first_subobject(ty)?.map(|index| vec![index]);
                let mut bound = 0;
                for item in items {
                    let mut path = if item.node.designation.is_empty() {
                        cursor.take().ok_or_else(|| {
                            Error::new(item.span.start, "excess elements in initializer")
                        })?
                    } else {
                        self.designated_subobject(ty, &item.node.designation)?
                    };
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
                                && (!empty || (!static_storage && self.gnu_flexible_arrays()))
                            {
                                let message = if !static_storage {
                                    "flexible array initialization requires static storage on this target"
                                } else if depth == 1
                                    && flexible.is_none()
                                    && static_storage
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
                            TypeKind::Array { .. } | TypeKind::Record(_)
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
                                static_storage,
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
        if matches!(self.unit.resolve(ty)?.kind, TypeKind::Record(_)) {
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
            TypeKind::Array { .. } => Some(0),
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
    ) -> Result<Vec<u64>, Error> {
        let mut path = Vec::new();
        for designator in designators {
            let offset = designator.span.start;
            let ty = self.subobject(root, &path, offset)?;
            match &designator.node {
                ast::Designator::Index(expression) => {
                    if !matches!(self.unit.resolve(&ty)?.kind, TypeKind::Array { .. }) {
                        return Err(Error::new(offset, "array designator requires an array"));
                    }
                    path.push(self.eval(expression)?.as_u64()?);
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

    fn static_initializer(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<ConstantKind, Error> {
        self.enter_expression(expression.span.start)?;
        let result = self.static_initializer_inner(expression);
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
            ast::Expression::Identifier(identifier) => {
                if self.unit.constants.contains_key(&identifier.node.name) {
                    return Ok(ConstantKind::Arithmetic);
                }
                let ty = self.expression_type(expression)?;
                if matches!(
                    self.unit.resolve(&ty)?.kind,
                    TypeKind::Array { .. } | TypeKind::Function(_)
                ) && self.object_has_static_storage(&identifier.node.name)
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
                ast::UnaryOperator::Plus
                | ast::UnaryOperator::Minus
                | ast::UnaryOperator::Complement
                | ast::UnaryOperator::Negate => {
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
                if let Ok(value) = self.eval_arithmetic(&binary.node.lhs)
                    && ((binary.node.operator.node == Op::LogicalAnd && !value.truth())
                        || (binary.node.operator.node == Op::LogicalOr && value.truth()))
                {
                    return Ok(ConstantKind::Arithmetic);
                }
                let right = self.static_initializer(&binary.node.rhs)?;
                if left == ConstantKind::Arithmetic && right == ConstantKind::Arithmetic {
                    self.eval_arithmetic(expression)?;
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
                let condition = self.eval_arithmetic(&conditional.node.condition)?;
                self.static_initializer(if condition.truth() {
                    &conditional.node.then_expression
                } else {
                    &conditional.node.else_expression
                })
            }
            ast::Expression::GenericSelection(selection) => {
                let selected = self.generic_expression(selection)?;
                self.static_initializer(selected)
            }
            _ => Err(invalid()),
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
            ast::Expression::Identifier(identifier) => {
                if self.object_has_static_storage(&identifier.node.name) {
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
