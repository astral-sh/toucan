//! Bounded integrity checks for the public retained graph. Arenas and owned edge
//! lists are visited once; record and expression references are never expanded.

use toucan::semantic::checked::*;
use toucan::semantic::{Analysis, TranslationUnit, Type, TypeKind};

pub(super) fn check(analysis: &Analysis, source: &str) {
    let unit = analysis.unit();
    let code = analysis.checked().expect("retention requested");
    let mut coverage = vec![0u8; code.occurrences().len()];
    let mut scope_membership = vec![0u8; code.declarations().len()];
    let mut type_operand_membership = vec![0u8; code.type_operands().len()];
    for (id, occurrence) in code.occurrences() {
        source_span(source, occurrence.source());
        if let Some(owner) = occurrence.type_owner() {
            let owner_node = code.occurrence(owner).unwrap();
            assert_eq!(owner_node.type_owner(), Some(owner));
            assert!(matches!(
                owner_node.kind(),
                OccurrenceKind::Declaration
                    | OccurrenceKind::Parameter
                    | OccurrenceKind::Field
                    | OccurrenceKind::Function
                    | OccurrenceKind::TypeName
            ));
        }
        for operand in occurrence.type_operands() {
            assert_eq!(code.type_operand(*operand).unwrap().owner(), id);
            type_operand_membership[operand.index()] += 1;
        }
    }
    for (id, operand) in code.type_operands() {
        assert_eq!(type_operand_membership[id.index()], 1);
        let occurrence = code.occurrence(operand.occurrence()).unwrap();
        assert_eq!(occurrence.kind(), OccurrenceKind::TypeOf);
        assert_eq!(occurrence.type_owner(), Some(operand.owner()));
        assert!(code.occurrence(operand.owner()).is_some());
        assert!(code.scope(operand.scope()).is_some());
        assert_eq!(coverage[operand.occurrence().index()] & 8, 0);
        coverage[operand.occurrence().index()] |= 8;
        match operand.input() {
            TypeOperandInput::Expression(value) => expression_use(code, value),
            TypeOperandInput::TypeName {
                occurrence,
                type_use,
            } => {
                assert_eq!(
                    code.occurrence(*occurrence).unwrap().kind(),
                    OccurrenceKind::TypeName
                );
                assert!(code.type_use(*type_use).is_some());
            }
            _ => panic!("new type operand needs invariant coverage"),
        }
    }
    for (id, scope) in code.scopes() {
        source_span(source, scope.source());
        if let Some(parent) = scope.parent() {
            assert!(code.scope(parent).is_some());
            assert_ne!(id, parent);
        }
        let mut previous = 0;
        for site in scope.declarations() {
            let declaration = code.declaration(*site).unwrap();
            assert_eq!(declaration.scope(), id);
            scope_membership[site.index()] += 1;
            let start = code
                .occurrence(declaration.occurrence())
                .unwrap()
                .source()
                .range()
                .start;
            assert!(previous <= start);
            previous = start;
        }
    }
    for (id, entity) in code.entities() {
        if let Some(declaration) = entity.declaration() {
            assert!(unit.declarations.get(declaration).is_some());
        }
        if let Some(body) = entity.body() {
            assert_eq!(code.body(body).unwrap().entity(), id);
        }
        match entity.kind() {
            EntityKind::Record(record) => {
                assert!(unit.records.get(record).is_some());
            }
            EntityKind::Enum(enumeration) => {
                assert!(unit.enums.get(enumeration).is_some());
            }
            EntityKind::Field { record, index } => {
                field(unit, record, index);
            }
            EntityKind::Enumerator {
                enumeration,
                variant,
            } => {
                assert!(unit.enums[enumeration].variants.get(variant).is_some());
            }
            EntityKind::Variable
            | EntityKind::Function
            | EntityKind::Typedef
            | EntityKind::Parameter => {}
            _ => panic!("new entity kind needs invariant coverage"),
        }
    }
    for (id, declaration) in code.declarations() {
        assert_eq!(scope_membership[id.index()], 1);
        assert!(code.entity(declaration.entity()).is_some());
        assert!(code.scope(declaration.scope()).is_some());
        assert!(code.occurrence(declaration.occurrence()).is_some());
        assert!(code.ty(declaration.ty()).is_some());
        assert_eq!(
            code.type_use(declaration.type_use()).unwrap().shape(),
            declaration.ty()
        );
        if let Some(ty) = declaration.declared_type_use() {
            assert!(code.type_use(ty).is_some());
        }
        if let Some(span) = declaration.name_source() {
            source_span(source, span);
        }
        if let Some(body) = declaration.body() {
            assert_eq!(code.body(body).unwrap().declaration(), id);
        }
        if let Some(initializer) = declaration.initializer() {
            assert_eq!(
                code.initializer(initializer).unwrap().ty(),
                declaration.ty()
            );
        }
    }
    for attribute in code.diagnostic_attributes() {
        source_span(source, attribute.source());
        let declaration = code.declaration(attribute.declaration()).unwrap();
        assert_eq!(
            code.entity(declaration.entity()).unwrap().kind(),
            EntityKind::Function
        );
    }
    for (_, ty) in code.types() {
        type_shape(unit, ty);
    }
    for (_, ty) in code.type_uses() {
        let shape = code.ty(ty.shape()).unwrap();
        for function in ty.functions() {
            let mut function_type = shape;
            for step in function.path() {
                function_type = type_step(unit, function_type, step);
            }
            assert!(matches!(
                unit.resolve(function_type).unwrap().kind,
                TypeKind::Function(_)
            ));
            assert!(code.scope(function.scope()).is_some());
            for parameter in function.parameters() {
                let declaration = code.declaration(*parameter).unwrap();
                assert_eq!(declaration.scope(), function.scope());
                assert_eq!(
                    code.entity(declaration.entity()).unwrap().kind(),
                    EntityKind::Parameter
                );
            }
        }
        for extent in ty.extents() {
            assert!(code.bound(extent.bound()).is_some());
            let mut element = shape;
            for step in extent.path() {
                element = type_step(unit, element, step);
            }
            let kind = &unit.resolve(element).unwrap().kind;
            // A compatible conversion to a fixed-bound pointer can retain its
            // source runtime extent; the fixed destination does not erase it.
            assert!(matches!(
                kind,
                TypeKind::VariableArray { .. } | TypeKind::Array { .. }
            ));
        }
    }
    for (_, bound) in code.bounds() {
        source_span(source, bound.source());
        assert!(code.scope(bound.scope()).is_some());
        if let Some(owner) = bound.owner() {
            assert!(code.occurrence(owner).is_some());
        }
        match bound.value() {
            BoundValue::Expression(expression) | BoundValue::Constant { expression, .. } => {
                assert!(code.expression(*expression).is_some());
            }
            BoundValue::PrototypeStar => {}
            BoundValue::Composite { inputs, selection } => {
                if let Some(expression) = selection {
                    assert!(code.expression(*expression).is_some());
                }
                for input in inputs {
                    match input {
                        BoundInput::Runtime(id) => {
                            assert!(code.bound(*id).is_some());
                        }
                        BoundInput::Constant(_) | BoundInput::Unspecified => {}
                        _ => panic!("new bound input needs invariant coverage"),
                    }
                }
            }
            _ => panic!("new bound value needs invariant coverage"),
        }
    }
    for reference in code.references() {
        source_span(source, reference.source());
        assert!(code.entity(reference.target()).is_some());
        assert!(code.scope(reference.scope()).is_some());
    }
    for (_, operand) in code.assignment_conversions() {
        expression_use(code, operand);
    }
    for (_, expression) in code.expressions() {
        assert!(code.occurrence(expression.occurrence()).is_some());
        assert!(code.scope(expression.scope()).is_some());
        assert!(code.ty(expression.ty()).is_some());
        assert_eq!(
            code.type_use(expression.type_use()).unwrap().shape(),
            expression.ty()
        );
        if let Some(ty) = expression.type_name_use() {
            assert!(code.type_use(ty).is_some());
        }
        if let Some(name) = expression.type_name() {
            assert_eq!(
                code.occurrence(name).unwrap().kind(),
                OccurrenceKind::TypeName
            );
        }
        match expression.kind() {
            ExprKind::Integer(_) | ExprKind::Float { .. } | ExprKind::String(_) => {}
            ExprKind::Name(entity) => {
                assert!(code.entity(*entity).is_some());
            }
            ExprKind::Unary {
                operand,
                computation_type,
                write_back,
                ..
            } => {
                expression_use(code, operand);
                for ty in [computation_type, write_back].into_iter().flatten() {
                    assert!(code.ty(*ty).is_some());
                }
            }
            ExprKind::AddressIndirection {
                indirection,
                pointer,
            } => {
                assert!(code.occurrence(*indirection).is_some());
                expression_use(code, pointer);
            }
            ExprKind::Binary {
                left,
                right,
                computation_type,
                write_back,
                ..
            } => {
                expression_use(code, left);
                expression_use(code, right);
                for ty in [computation_type, write_back].into_iter().flatten() {
                    assert!(code.ty(*ty).is_some());
                }
            }
            ExprKind::Cast { destination, value } => {
                assert!(code.ty(*destination).is_some());
                expression_use(code, value);
            }
            ExprKind::Conditional {
                condition,
                then_value,
                else_value,
            } => {
                for operand in [condition, then_value, else_value] {
                    expression_use(code, operand);
                }
            }
            ExprKind::Member {
                base,
                indirect,
                fields,
            } => {
                expression_use(code, base);
                let mut ty = code.ty(base.effective_type()).unwrap();
                if *indirect {
                    let TypeKind::Pointer(pointee) = &unit.resolve(ty).unwrap().kind else {
                        panic!("arrow base is not a pointer")
                    };
                    ty = pointee;
                }
                field_path(unit, ty, fields);
            }
            ExprKind::Call {
                callee,
                direct_callee,
                arguments,
            } => {
                expression_use(code, callee);
                if let Some(entity) = direct_callee {
                    assert!(code.entity(*entity).is_some());
                }
                for argument in arguments {
                    expression_use(code, argument);
                }
            }
            ExprKind::BuiltinCall {
                callee_occurrence,
                arguments,
                ..
            } => {
                assert!(code.occurrence(*callee_occurrence).is_some());
                for argument in arguments {
                    expression_use(code, argument);
                }
            }
            ExprKind::VaArg {
                list,
                requested_type,
            } => {
                expression_use(code, list);
                assert!(code.ty(*requested_type).is_some());
            }
            ExprKind::SizeOfType(ty) | ExprKind::AlignOf(ty) => {
                assert!(code.ty(*ty).is_some());
            }
            ExprKind::SizeOfValue { operand, .. } => expression_use(code, operand),
            ExprKind::OffsetOf { record, members } => {
                let mut ty = code.ty(*record).unwrap();
                for member in members {
                    match member {
                        OffsetMember::Field(path) => ty = field_path(unit, ty, path),
                        OffsetMember::Index(expression) => {
                            assert!(code.expression(*expression).is_some());
                            ty = array_element(unit, ty);
                        }
                        _ => panic!("new offsetof member needs invariant coverage"),
                    }
                }
            }
            ExprKind::Generic {
                control,
                arms,
                selected,
            } => {
                expression_use(code, control);
                assert!(arms.get(*selected).is_some());
                for arm in arms {
                    assert!(code.expression(arm.expression()).is_some());
                    if let Some(ty) = arm.ty() {
                        assert!(code.ty(ty).is_some());
                    }
                }
            }
            ExprKind::Comma(operands) => {
                for operand in operands {
                    expression_use(code, operand);
                }
            }
            ExprKind::CompoundLiteral { initializer } => {
                assert!(code.initializer(*initializer).is_some());
            }
            ExprKind::StatementExpression { body, result } => {
                assert!(code.statement(*body).is_some());
                if let Some(result) = result {
                    expression_use(code, result);
                }
            }
            _ => panic!("new expression kind needs invariant coverage"),
        }
    }
    for (_, initializer) in code.initializers() {
        assert!(code.occurrence(initializer.occurrence()).is_some());
        assert!(code.scope(initializer.scope()).is_some());
        let root = code.ty(initializer.ty()).unwrap();
        match initializer.kind() {
            InitializerKind::Expression(assignment) => {
                assert!(code.assignment(*assignment).is_some());
            }
            InitializerKind::String {
                literal,
                copied_units,
                includes_implicit_terminator,
                ..
            } => {
                let ExprKind::String(string) = code.expression(*literal).unwrap().kind() else {
                    panic!("string initializer needs string expression")
                };
                assert!(*copied_units <= string.code_units.len() as u64);
                assert_eq!(
                    *includes_implicit_terminator,
                    *copied_units == string.code_units.len() as u64
                );
            }
            InitializerKind::List {
                entries,
                union_member,
                ..
            } => {
                if let Some(member) = union_member {
                    let TypeKind::Record(record) = unit.resolve(root).unwrap().kind else {
                        panic!("union initializer needs record")
                    };
                    field(unit, record, *member);
                }
                for entry in entries {
                    assert!(code.occurrence(entry.occurrence()).is_some());
                    for designator in entry.designators() {
                        assert!(code.occurrence(*designator).is_some());
                    }
                    assert!(code.initializer(entry.initializer()).is_some());
                    let mut ty = root;
                    for step in entry.path() {
                        match step {
                            Subobject::Field {
                                record,
                                field: index,
                            } => {
                                assert!(
                                    matches!(unit.resolve(ty).unwrap().kind, TypeKind::Record(id) if id == *record)
                                );
                                ty = &field(unit, *record, *index).ty;
                            }
                            Subobject::Index { index, expression } => {
                                array_index(unit, ty, *index);
                                ty = array_element(unit, ty);
                                if let Some(expression) = expression {
                                    assert!(code.expression(*expression).is_some());
                                }
                            }
                            Subobject::Range {
                                start,
                                end,
                                from,
                                to,
                            } => {
                                assert!(start <= end);
                                array_index(unit, ty, *end);
                                ty = array_element(unit, ty);
                                assert!(code.expression(*from).is_some());
                                assert!(code.expression(*to).is_some());
                            }
                            _ => panic!("new initializer path needs invariant coverage"),
                        }
                    }
                }
            }
            _ => panic!("unfinished or unhandled initializer"),
        }
    }
    for (id, body) in code.bodies() {
        assert_eq!(code.entity(body.entity()).unwrap().body(), Some(id));
        assert_eq!(
            code.declaration(body.declaration()).unwrap().body(),
            Some(id)
        );
        assert!(code.statement(body.statement()).is_some());
        assert!(code.scope(body.scope()).is_some());
        assert!(code.ty(body.signature()).is_some());
        for parameter in body.parameters() {
            assert!(code.declaration(*parameter).is_some());
        }
    }
    for (_, group) in code.declaration_groups() {
        assert!(code.occurrence(group.occurrence()).is_some());
        for site in group.declarations() {
            assert!(code.declaration(*site).is_some());
        }
    }
    for (_, assertion) in code.assertions() {
        assert!(code.occurrence(assertion.occurrence()).is_some());
        assert!(code.occurrence(assertion.message_occurrence()).is_some());
        assert!(code.scope(assertion.scope()).is_some());
        expression_use(code, assertion.condition());
    }
    for (_, statement) in code.statements() {
        assert!(code.occurrence(statement.occurrence()).is_some());
        assert!(code.scope(statement.scope()).is_some());
        match statement.kind() {
            StatementKind::Block(items) => {
                for item in items {
                    match item {
                        BlockItem::Declaration(id) => {
                            assert!(code.declaration_group(*id).is_some());
                        }
                        BlockItem::Statement(id) => {
                            assert!(code.statement(*id).is_some());
                        }
                        BlockItem::Assertion(id) => {
                            assert!(code.assertion(*id).is_some());
                        }
                        _ => panic!("new block item needs invariant coverage"),
                    }
                }
            }
            StatementKind::Expression(value) | StatementKind::Return(value) => {
                if let Some(value) = value {
                    expression_use(code, value);
                }
            }
            StatementKind::If {
                condition,
                then_statement,
                else_statement,
            } => {
                expression_use(code, condition);
                assert!(code.statement(*then_statement).is_some());
                if let Some(other) = else_statement {
                    assert!(code.statement(*other).is_some());
                }
            }
            StatementKind::While { condition, body }
            | StatementKind::DoWhile { body, condition } => {
                expression_use(code, condition);
                assert!(code.statement(*body).is_some());
            }
            StatementKind::For {
                initializer,
                condition,
                step,
                body,
            } => {
                match initializer {
                    ForInitializer::Empty => {}
                    ForInitializer::Expression(value) => expression_use(code, value),
                    ForInitializer::Declaration(id) => {
                        assert!(code.declaration_group(*id).is_some());
                    }
                    ForInitializer::Assertion(id) => {
                        assert!(code.assertion(*id).is_some());
                    }
                    _ => panic!("new for initializer needs invariant coverage"),
                }
                for value in [condition, step].into_iter().flatten() {
                    expression_use(code, value);
                }
                assert!(code.statement(*body).is_some());
            }
            StatementKind::Switch { expression, body } => {
                expression_use(code, expression);
                assert!(code.statement(*body).is_some());
            }
            StatementKind::Labeled {
                occurrence,
                label,
                statement,
            } => {
                assert!(code.occurrence(*occurrence).is_some());
                assert!(code.statement(*statement).is_some());
                match label {
                    Label::Identifier(_) => {}
                    Label::Case {
                        expression, switch, ..
                    } => {
                        expression_use(code, expression);
                        assert!(matches!(
                            code.statement(*switch).unwrap().kind(),
                            StatementKind::Switch { .. }
                        ));
                    }
                    Label::CaseRange {
                        low, high, switch, ..
                    } => {
                        expression_use(code, low);
                        expression_use(code, high);
                        assert!(matches!(
                            code.statement(*switch).unwrap().kind(),
                            StatementKind::Switch { .. }
                        ));
                    }
                    Label::Default { switch } => {
                        assert!(matches!(
                            code.statement(*switch).unwrap().kind(),
                            StatementKind::Switch { .. }
                        ));
                    }
                    _ => panic!("new label needs invariant coverage"),
                }
            }
            StatementKind::Goto { target, .. } => {
                assert!(code.statement(target.expect("resolved goto")).is_some());
            }
            StatementKind::Break { target } | StatementKind::Continue { target } => {
                assert!(code.statement(*target).is_some());
            }
            StatementKind::Assembly(assembly) => {
                assert!(code.occurrence(assembly.template().occurrence()).is_some());
                for text in assembly.clobbers() {
                    assert!(code.occurrence(text.occurrence()).is_some());
                }
                for operand in assembly.outputs().iter().chain(assembly.inputs()) {
                    assert!(code.occurrence(operand.occurrence()).is_some());
                    assert!(
                        code.occurrence(operand.constraints().occurrence())
                            .is_some()
                    );
                    for value in [operand.place(), operand.value()].into_iter().flatten() {
                        expression_use(code, value);
                    }
                    for alternative in operand.alternatives() {
                        if let Some(index) = alternative.matching() {
                            assert!(assembly.outputs().get(index).is_some());
                        }
                    }
                }
            }
            _ => panic!("unfinished or unhandled statement"),
        }
    }
    for row in code.expression_coverage() {
        let occurrence = code.occurrence(row.occurrence()).unwrap();
        assert_eq!(coverage[row.occurrence().index()] & 1, 0);
        coverage[row.occurrence().index()] |= 1;
        match row.status() {
            ExpressionStatus::Typed(id) => {
                assert_eq!(code.expression(*id).unwrap().occurrence(), row.occurrence())
            }
            ExpressionStatus::BuiltinCallee(id) => assert!(
                matches!(code.expression(*id).unwrap().kind(), ExprKind::BuiltinCall { callee_occurrence, .. } if *callee_occurrence == row.occurrence())
            ),
            ExpressionStatus::CanceledIndirection(id) => assert!(
                matches!(code.expression(*id).unwrap().kind(), ExprKind::AddressIndirection { indirection, .. } if *indirection == row.occurrence())
            ),
            ExpressionStatus::AttributeArgument => assert!(occurrence.attribute_argument()),
            ExpressionStatus::ParserInserted => assert!(occurrence.source().synthetic()),
            _ => panic!("missing or unhandled expression coverage"),
        }
    }
    for row in code.initializer_coverage() {
        let occurrence = code.occurrence(row.occurrence()).unwrap();
        assert_eq!(coverage[row.occurrence().index()] & 4, 0);
        coverage[row.occurrence().index()] |= 4;
        match row.status() {
            InitializerStatus::Retained(id) => assert_eq!(
                code.initializer(*id).unwrap().occurrence(),
                row.occurrence()
            ),
            InitializerStatus::AttributeArgument => assert!(occurrence.attribute_argument()),
            InitializerStatus::ParserInserted => assert!(occurrence.source().synthetic()),
            _ => panic!("missing or unhandled initializer coverage"),
        }
    }
    for row in code.statement_coverage() {
        let occurrence = code.occurrence(row.occurrence()).unwrap();
        assert_eq!(coverage[row.occurrence().index()] & 2, 0);
        coverage[row.occurrence().index()] |= 2;
        match row.status() {
            StatementStatus::Checked(id) => {
                assert_eq!(code.statement(*id).unwrap().occurrence(), row.occurrence())
            }
            StatementStatus::AttributeArgument => assert!(occurrence.attribute_argument()),
            StatementStatus::ParserInserted => assert!(occurrence.source().synthetic()),
            _ => panic!("missing or unhandled statement coverage"),
        }
    }
    for (id, occurrence) in code.occurrences() {
        match occurrence.kind() {
            OccurrenceKind::Expression => assert_ne!(coverage[id.index()] & 1, 0),
            OccurrenceKind::Statement => assert_ne!(coverage[id.index()] & 2, 0),
            OccurrenceKind::Initializer => assert_ne!(coverage[id.index()] & 4, 0),
            OccurrenceKind::TypeOf if !occurrence.attribute_argument() => {
                assert_ne!(coverage[id.index()] & 8, 0)
            }
            _ => {}
        }
    }
}

fn source_span(source: &str, span: &SourceSpan) {
    let range = span.range();
    assert!(source.get(range.clone()).is_some());
    for fragment in span.fragments() {
        assert!(source.get(fragment.clone()).is_some());
        assert!(range.start <= fragment.start && fragment.end <= range.end);
    }
}
fn expression_use(code: &CheckedCode, operand: &ExprUse) {
    assert!(code.expression(operand.expression()).is_some());
    assert!(code.ty(operand.effective_type()).is_some());
    assert_eq!(
        code.type_use(operand.type_use()).unwrap().shape(),
        operand.effective_type()
    );
    for step in operand.conversions() {
        assert!(code.ty(step.target_type()).is_some());
    }
    if let Some(last) = operand.conversions().last() {
        assert_eq!(last.target_type(), operand.effective_type());
    }
}
fn field(unit: &TranslationUnit, record: usize, field: usize) -> &toucan::semantic::Field {
    &unit.records[record].fields.as_ref().unwrap()[field]
}
fn field_path<'a>(unit: &'a TranslationUnit, mut ty: &'a Type, path: &[usize]) -> &'a Type {
    assert!(!path.is_empty());
    for index in path {
        let TypeKind::Record(record) = unit.resolve(ty).unwrap().kind else {
            panic!("member path needs record")
        };
        ty = &field(unit, record, *index).ty;
    }
    ty
}
fn array_element<'a>(unit: &'a TranslationUnit, ty: &'a Type) -> &'a Type {
    match &unit.resolve(ty).unwrap().kind {
        TypeKind::Array { element, .. } | TypeKind::VariableArray { element } => element,
        _ => panic!("array path needs array"),
    }
}
fn array_index(unit: &TranslationUnit, ty: &Type, index: u64) {
    if let TypeKind::Array {
        length: Some(length),
        ..
    } = unit.resolve(ty).unwrap().kind
    {
        assert!(index < length);
    }
}
fn type_step<'a>(unit: &'a TranslationUnit, ty: &'a Type, step: &TypeStep) -> &'a Type {
    match (step, &unit.resolve(ty).unwrap().kind) {
        (TypeStep::Pointer, TypeKind::Pointer(pointee)) => pointee,
        (
            TypeStep::Element,
            TypeKind::Array { element, .. } | TypeKind::VariableArray { element },
        ) => element,
        (TypeStep::Return, TypeKind::Function(function)) => &function.return_type,
        (TypeStep::Parameter(index), TypeKind::Function(function)) => {
            &function.parameters[*index].ty
        }
        _ => panic!("type-use path does not match its shape"),
    }
}
fn type_shape(unit: &TranslationUnit, root: &Type) {
    let mut pending = vec![root];
    while let Some(ty) = pending.pop() {
        match &ty.kind {
            TypeKind::Pointer(pointee) => pending.push(pointee),
            TypeKind::Array { element, .. } | TypeKind::VariableArray { element } => {
                pending.push(element)
            }
            TypeKind::Function(function) => {
                pending.push(&function.return_type);
                pending.extend(function.parameters.iter().map(|parameter| &parameter.ty));
            }
            TypeKind::Record(index) => {
                assert!(unit.records.get(*index).is_some());
            }
            TypeKind::Enum(index) => {
                assert!(unit.enums.get(*index).is_some());
            }
            TypeKind::Typedef(_) => {
                assert!(unit.resolve(ty).is_ok());
            }
            TypeKind::Void | TypeKind::Bool | TypeKind::Integer(_) | TypeKind::Float(_) => {}
        }
    }
}
