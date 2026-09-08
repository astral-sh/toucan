//! Immutable views of retained semantic data.

use super::initializer::Entry;
use super::*;
use crate::{DecodedString, IntegerValue};

impl SourceSpan {
    /// Returns the covering byte range in the original preprocessed input.
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }
    /// Returns disjoint or reordered pieces, or an empty slice for a contiguous span.
    pub fn fragments(&self) -> &[Range<usize>] {
        &self.fragments
    }
    /// Whether this span consists entirely of parser-inserted text.
    pub fn synthetic(&self) -> bool {
        self.synthetic
    }
}

impl Occurrence {
    /// Whether this occurrence belongs to attribute metadata rather than C evaluation.
    pub fn attribute_argument(&self) -> bool {
        self.attribute_argument
    }
    /// The written syntactic role before semantic adaptation.
    pub fn kind(&self) -> OccurrenceKind {
        self.kind
    }
    /// Original preprocessed byte ranges, including disjoint adapter fragments.
    pub fn source(&self) -> &SourceSpan {
        &self.source
    }
}

impl Scope {
    /// The enclosing lexical scope; absent only for file scope.
    pub fn parent(&self) -> Option<ScopeId> {
        self.parent
    }
    pub fn kind(&self) -> ScopeKind {
        self.kind
    }
    /// The written extent of this lexical scope.
    pub fn source(&self) -> &SourceSpan {
        &self.source
    }
    /// Declaration sites in this scope, sorted by their written source position.
    pub fn declarations(&self) -> &[SiteId] {
        &self.declarations
    }
}

impl Entity {
    /// Merged explicit alignment of the object or function.
    pub fn alignment(&self) -> crate::DeclarationAlignment {
        self.alignment
    }

    /// Whether any compatible declaration carries `returns_twice`. Later annotations
    /// do not retroactively describe the compiler effects of earlier call sites.
    pub fn returns_twice(&self) -> bool {
        self.returns_twice
    }
    /// Final symbol binding after all compatible declarations, including block externs.
    pub fn symbol_binding(&self) -> crate::SymbolBinding {
        self.symbol_binding
    }
    /// Returns the definition body, including when this entity has earlier prototypes.
    pub fn body(&self) -> Option<BodyId> {
        self.body
    }
    /// The original C spelling, absent for anonymous entities.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    /// The C namespace role and any canonical record, enum, or member identity.
    pub fn kind(&self) -> EntityKind {
        self.kind
    }
    /// Returns the canonical index in `TranslationUnit::declarations`, when present.
    pub fn declaration(&self) -> Option<usize> {
        self.declaration
    }
    /// Object lifetime, independent of internal or external linkage.
    pub fn storage(&self) -> Storage {
        self.storage
    }
    /// Whether declarations of this entity share identity across scopes or files.
    pub fn linkage(&self) -> Linkage {
        self.linkage
    }
}

impl DeclarationSite {
    /// Alignment written on this declaration, before inheriting visible requirements.
    pub fn alignment(&self) -> crate::DeclarationAlignment {
        self.alignment
            .as_ref()
            .map_or_else(Default::default, |alignment| alignment.written)
    }
    /// Effective alignment at this declaration's scope and source position.
    /// A later declaration may have different inherited requirements.
    pub fn effective_alignment(&self) -> crate::DeclarationAlignment {
        self.alignment
            .as_ref()
            .map_or_else(Default::default, |alignment| alignment.effective)
    }

    /// The returns-twice property merged when this function was declared.
    /// This is a declaration fact, not the result of compiler call lowering.
    pub fn returns_twice(&self) -> bool {
        self.returns_twice
    }
    /// The explicit attribute, when written on this declaration.
    pub fn returns_twice_attribute(&self) -> Option<&SourceSpan> {
        self.returns_twice_attribute.as_ref()
    }
    /// Effective symbol binding at this declaration. Later declarations may change the entity.
    pub fn symbol_binding(&self) -> crate::SymbolBinding {
        self.symbol_binding
    }
    /// The explicit GNU weak attribute, if written on this declaration.
    pub fn weak_attribute(&self) -> Option<&SourceSpan> {
        self.weak_attribute.as_ref()
    }
    /// The function body supplied by this particular declaration, if any.
    pub fn body(&self) -> Option<BodyId> {
        self.body
    }
    /// The written initializer supplied by this declaration.
    pub fn initializer(&self) -> Option<InitializerId> {
        self.initializer
    }
    /// The identifier token, preserved through parenthesized declarators.
    pub fn name_source(&self) -> Option<&SourceSpan> {
        self.name_source.as_ref()
    }
    /// The declared identity, shared by compatible redeclarations.
    pub fn entity(&self) -> EntityId {
        self.entity
    }
    /// The written declarator, parameter, or member occurrence.
    pub fn occurrence(&self) -> OccurrenceId {
        self.occurrence
    }
    /// The lexical scope that contains this declaration.
    pub fn scope(&self) -> ScopeId {
        self.scope
    }
    /// Returns the final effective declaration type, after completion and adjustment.
    pub fn ty(&self) -> TypeId {
        self.ty
    }
    /// The object storage duration represented by this declaration.
    pub fn storage(&self) -> Storage {
        self.storage
    }
    pub fn linkage(&self) -> Linkage {
        self.linkage
    }
    /// Whether this declaration uses the C `register` storage-class specifier.
    pub fn register(&self) -> bool {
        self.register
    }
    /// Whether this declaration supplies a definition rather than only a declaration.
    pub fn definition(&self) -> bool {
        self.definition
    }
    /// Returns extra storage allocated by a supported flexible-array initializer.
    pub fn flexible_array_storage(&self) -> Option<&FlexibleArrayStorage> {
        self.flexible_array_storage.as_ref()
    }
}

impl ConversionStep {
    /// The C conversion applied at this step.
    pub fn kind(&self) -> Conversion {
        self.kind
    }
    /// The operand type after this conversion.
    pub fn target_type(&self) -> TypeId {
        self.target_type
    }
}

impl ExprUse {
    /// The written expression before conversions imposed by this use.
    pub fn expression(&self) -> ExprId {
        self.expression
    }
    /// Returns the operand type after the recorded conversions.
    pub fn effective_type(&self) -> TypeId {
        self.effective_type
    }
    /// Returns whether the operand is read, written, or unevaluated.
    pub fn context(&self) -> UseContext {
        self.context
    }
    /// Returns the conversions in application order for this operand use.
    pub fn conversions(&self) -> &[ConversionStep] {
        &self.conversions
    }
}

impl Expression {
    /// Atomic store/update performed by this expression, with C's sequentially
    /// consistent ordering. Loads appear as AtomicLoad operand conversions.
    pub fn atomic_access(&self) -> Option<super::AtomicAccess> {
        self.atomic_access
    }
    /// Whether this expression designates a vector lane, whose address is unavailable in Clang profiles.
    pub fn is_vector_element(&self) -> bool {
        self.vector_element
    }

    /// The written expression occurrence.
    pub fn occurrence(&self) -> OccurrenceId {
        self.occurrence
    }
    /// The lexical scope used to resolve names in this expression.
    pub fn scope(&self) -> ScopeId {
        self.scope
    }
    /// Returns the expression type before conversions imposed by its parent use.
    pub fn ty(&self) -> TypeId {
        self.ty
    }
    /// Whether the expression supplies a value, object location, or function designator.
    pub fn category(&self) -> ValueCategory {
        self.category
    }
    /// Returns the width when this expression designates a bitfield.
    pub fn bitfield(&self) -> Option<u64> {
        self.bitfield
    }
    /// Whether this expression designates an object declared `register`.
    pub fn register(&self) -> bool {
        self.register
    }
    /// The checked operation and its linked operand uses.
    pub fn kind(&self) -> &ExprKind {
        &self.kind
    }
}

impl GenericArm {
    /// The association type, or `None` for a default association.
    pub fn ty(&self) -> Option<TypeId> {
        self.ty
    }
    /// The checked arm expression, including unselected associations.
    pub fn expression(&self) -> ExprId {
        self.expression
    }
}

impl ExpressionCoverage {
    /// The parsed expression occurrence accounted for by this row.
    pub fn occurrence(&self) -> OccurrenceId {
        self.occurrence
    }
    /// Its typed node or explicit metadata/adapter classification.
    pub fn status(&self) -> &ExpressionStatus {
        &self.status
    }
}

impl Initializer {
    /// The written initializer or owning compound-literal occurrence.
    pub fn occurrence(&self) -> OccurrenceId {
        self.occurrence
    }
    /// The lexical scope in which this initializer was checked.
    pub fn scope(&self) -> ScopeId {
        self.scope
    }
    /// The completed destination type shape; runtime bounds belong to its owner.
    pub fn ty(&self) -> TypeId {
        self.ty
    }
    /// Whether static or thread storage requires constant-expression initialization.
    pub fn requires_constant(&self) -> bool {
        self.requires_constant
    }
    /// Extra storage allocated for a supported flexible-array initializer.
    pub fn flexible_array_storage(&self) -> Option<&FlexibleArrayStorage> {
        self.flexible_array_storage.as_ref()
    }
    /// Sparse initialization structure; written order does not fix evaluation order.
    pub fn kind(&self) -> &InitializerKind {
        &self.kind
    }
}

impl Entry {
    /// The written initializer-list item.
    pub fn occurrence(&self) -> OccurrenceId {
        self.occurrence
    }
    /// Returns the written designator occurrences in their source order.
    pub fn designators(&self) -> &[OccurrenceId] {
        &self.designators
    }
    /// Returns a sparse subobject path, including expanded anonymous-member steps.
    pub fn path(&self) -> &[Subobject] {
        &self.path
    }
    /// The initializer applied to the selected subobjects.
    pub fn initializer(&self) -> InitializerId {
        self.initializer
    }
}

impl InitializerCoverage {
    /// The parsed initializer occurrence accounted for by this row.
    pub fn occurrence(&self) -> OccurrenceId {
        self.occurrence
    }
    /// Its retained root or explicit metadata/adapter classification.
    pub fn status(&self) -> &InitializerStatus {
        &self.status
    }
}

impl FunctionBody {
    /// The function identity shared with earlier compatible prototypes.
    pub fn entity(&self) -> EntityId {
        self.entity
    }
    /// The declaration site that supplies this definition.
    pub fn declaration(&self) -> SiteId {
        self.declaration
    }
    /// The checked outer statement of the function body.
    pub fn statement(&self) -> StatementId {
        self.statement
    }
    /// The definition scope containing the named parameters.
    pub fn scope(&self) -> ScopeId {
        self.scope
    }
    /// The body's adjusted local parameter types and calling convention.
    /// For identifier-list definitions, use `old_style()` for incoming argument
    /// types and the declaration site's type for the canonical calling interface.
    pub fn signature(&self) -> TypeId {
        self.signature
    }
    /// Named parameter declaration sites in signature order.
    pub fn parameters(&self) -> &[SiteId] {
        &self.parameters
    }
    /// Entry types and source declarations for an identifier-list definition.
    pub fn old_style(&self) -> Option<&OldStyleDefinition> {
        self.old_style.as_deref()
    }
}

impl OldStyleDefinition {
    /// Parameter declaration groups in written source order.
    pub fn declarations(&self) -> &[DeclarationGroupId] {
        &self.declarations
    }
    /// Incoming parameters in identifier-list argument order.
    pub fn parameters(&self) -> &[ParameterEntry] {
        &self.parameters
    }
    /// Ordering promised between different parameters' entry evaluations.
    pub fn evaluation_order(&self) -> ParameterEvaluationOrder {
        self.evaluation_order
    }
}
impl ParameterEntry {
    /// Identifier occurrence in the definition's parenthesized identifier list.
    pub fn identifier(&self) -> OccurrenceId {
        self.identifier
    }
    /// The original typed parameter declaration site.
    pub fn declaration(&self) -> SiteId {
        self.declaration
    }
    /// Incoming C type after promotions or adoption of an earlier prototype.
    pub fn incoming(&self) -> TypeUseId {
        self.incoming
    }
    /// Conversion into the local parameter; no atomic load is implied.
    pub fn conversions(&self) -> &[ConversionStep] {
        &self.conversions
    }
}

impl StatementCoverage {
    /// The parsed statement occurrence accounted for by this row.
    pub fn occurrence(&self) -> OccurrenceId {
        self.occurrence
    }
    /// Its checked node or explicit metadata/adapter classification.
    pub fn status(&self) -> &StatementStatus {
        &self.status
    }
}

impl Statement {
    /// The written statement occurrence.
    pub fn occurrence(&self) -> OccurrenceId {
        self.occurrence
    }
    /// The containing lexical scope, including a compound statement’s own scope.
    pub fn scope(&self) -> ScopeId {
        self.scope
    }
    /// The checked statement, including resolved jump targets.
    pub fn kind(&self) -> &StatementKind {
        &self.kind
    }
}

impl DeclarationGroup {
    /// The single written declaration containing these declarators.
    pub fn occurrence(&self) -> OccurrenceId {
        self.occurrence
    }
    /// Returns sites introduced by this declaration in its own lexical scope.
    pub fn declarations(&self) -> &[SiteId] {
        &self.declarations
    }
}

impl Assertion {
    /// The written static assertion.
    pub fn occurrence(&self) -> OccurrenceId {
        self.occurrence
    }
    /// The lexical scope used to check its condition.
    pub fn scope(&self) -> ScopeId {
        self.scope
    }
    /// Returns the checked translation-time condition; it has no runtime effect.
    pub fn condition(&self) -> &ExprUse {
        &self.condition
    }
    /// The evaluated integer value of the condition.
    pub fn value(&self) -> IntegerValue {
        self.value
    }
    /// The owned decoded assertion message.
    pub fn message(&self) -> &DecodedString {
        &self.message
    }
    /// The written string literal containing the message.
    pub fn message_occurrence(&self) -> OccurrenceId {
        self.message_occurrence
    }
}

impl Assembly {
    /// The decoded assembly template and its written occurrence.
    pub fn template(&self) -> &AssemblyText {
        &self.template
    }
    /// Whether this is basic assembly without extended operand lists.
    pub fn basic(&self) -> bool {
        self.basic
    }
    /// Whether the checker treats this assembly as volatile.
    pub fn volatile(&self) -> bool {
        self.volatile
    }
    /// Output operands in constraint numbering order.
    pub fn outputs(&self) -> &[AssemblyOperand] {
        &self.outputs
    }
    /// Input operands in constraint numbering order, after outputs.
    pub fn inputs(&self) -> &[AssemblyOperand] {
        &self.inputs
    }
    /// Written clobber names and their source occurrences.
    pub fn clobbers(&self) -> &[AssemblyText] {
        &self.clobbers
    }
}

impl AssemblyText {
    /// The written assembly string literal.
    pub fn occurrence(&self) -> OccurrenceId {
        self.occurrence
    }
    /// The decoded assembly string contents.
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl AssemblyOperand {
    /// The written assembly operand.
    pub fn occurrence(&self) -> OccurrenceId {
        self.occurrence
    }
    /// The optional symbolic operand name, without brackets.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    /// The decoded constraint string and its source occurrence.
    pub fn constraints(&self) -> &AssemblyText {
        &self.constraints
    }
    /// Whether an output constraint reads its previous value.
    pub fn read_write(&self) -> bool {
        self.read_write
    }
    /// Whether the checked operand is an integer constant expression.
    pub fn integer_constant(&self) -> bool {
        self.integer_constant
    }
    /// The checked alternatives in constraint order.
    pub fn alternatives(&self) -> &[AssemblyLocation] {
        &self.alternatives
    }
    /// The object location used by memory operands and output writes.
    pub fn place(&self) -> Option<&ExprUse> {
        self.place.as_ref()
    }
    /// The value supplied to register or immediate operands.
    pub fn value(&self) -> Option<&ExprUse> {
        self.value.as_ref()
    }
}

impl AssemblyLocation {
    /// Whether this alternative admits register placement.
    pub fn register(&self) -> bool {
        self.register
    }
    /// Whether this alternative admits a memory operand.
    pub fn memory(&self) -> bool {
        self.memory
    }
    /// Whether this alternative admits an immediate constant.
    pub fn immediate(&self) -> bool {
        self.immediate
    }
    /// Fixed register names required by the constraint.
    pub fn fixed(&self) -> &[String] {
        &self.fixed
    }
    /// The output operand index matched by this alternative.
    pub fn matching(&self) -> Option<usize> {
        self.matching
    }
}

impl Reference {
    /// The resolved typedef, tag, or member entity.
    pub fn target(&self) -> EntityId {
        self.target
    }
    /// The lexical scope used to resolve this reference.
    pub fn scope(&self) -> ScopeId {
        self.scope
    }
    /// The C namespace used by this written reference.
    pub fn kind(&self) -> ReferenceKind {
        self.kind
    }
    /// The identifier token that names the referenced entity.
    pub fn source(&self) -> &SourceSpan {
        &self.source
    }
}

impl CheckedCode {
    /// Looks up an owner-local `OccurrenceId`; out-of-range IDs return `None`.
    pub fn occurrence(&self, id: OccurrenceId) -> Option<&Occurrence> {
        self.occurrences.get(id.index())
    }
    /// Iterates in arena order, retaining each node's owner-local ID.
    pub fn occurrences(
        &self,
    ) -> impl ExactSizeIterator<Item = (OccurrenceId, &Occurrence)> + DoubleEndedIterator {
        self.occurrences
            .iter()
            .enumerate()
            .map(|(index, node)| (OccurrenceId(index as u32), node))
    }
    /// Looks up an owner-local `ScopeId`; out-of-range IDs return `None`.
    pub fn scope(&self, id: ScopeId) -> Option<&Scope> {
        self.scopes.get(id.index())
    }
    /// Iterates in arena order, retaining each node's owner-local ID.
    pub fn scopes(&self) -> impl ExactSizeIterator<Item = (ScopeId, &Scope)> + DoubleEndedIterator {
        self.scopes
            .iter()
            .enumerate()
            .map(|(index, node)| (ScopeId(index as u32), node))
    }
    /// Looks up an owner-local `EntityId`; out-of-range IDs return `None`.
    pub fn entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(id.index())
    }
    /// Iterates in arena order, retaining each node's owner-local ID.
    pub fn entities(
        &self,
    ) -> impl ExactSizeIterator<Item = (EntityId, &Entity)> + DoubleEndedIterator {
        self.entities
            .iter()
            .enumerate()
            .map(|(index, node)| (EntityId(index as u32), node))
    }
    /// Looks up an owner-local `SiteId`; out-of-range IDs return `None`.
    pub fn declaration(&self, id: SiteId) -> Option<&DeclarationSite> {
        self.declarations.get(id.index())
    }
    /// Iterates in arena order, retaining each node's owner-local ID.
    pub fn declarations(
        &self,
    ) -> impl ExactSizeIterator<Item = (SiteId, &DeclarationSite)> + DoubleEndedIterator {
        self.declarations
            .iter()
            .enumerate()
            .map(|(index, node)| (SiteId(index as u32), node))
    }
    /// Looks up an owner-local `TypeId`; out-of-range IDs return `None`.
    pub fn ty(&self, id: TypeId) -> Option<&Type> {
        self.types.get(id.index())
    }
    /// Iterates in arena order, retaining each node's owner-local ID.
    pub fn types(&self) -> impl ExactSizeIterator<Item = (TypeId, &Type)> + DoubleEndedIterator {
        self.types
            .iter()
            .enumerate()
            .map(|(index, node)| (TypeId(index as u32), node))
    }
    /// Looks up an owner-local `ExprId`; out-of-range IDs return `None`.
    pub fn expression(&self, id: ExprId) -> Option<&Expression> {
        self.expressions.get(id.index())
    }
    /// Iterates in arena order, retaining each node's owner-local ID.
    pub fn expressions(
        &self,
    ) -> impl ExactSizeIterator<Item = (ExprId, &Expression)> + DoubleEndedIterator {
        self.expressions
            .iter()
            .enumerate()
            .map(|(index, node)| (ExprId(index as u32), node))
    }
    /// Looks up an owner-local `AssignmentId`; out-of-range IDs return `None`.
    pub fn assignment(&self, id: AssignmentId) -> Option<&ExprUse> {
        self.assignment_conversions.get(id.index())
    }
    /// Iterates in arena order, retaining each node's owner-local ID.
    pub fn assignment_conversions(
        &self,
    ) -> impl ExactSizeIterator<Item = (AssignmentId, &ExprUse)> + DoubleEndedIterator {
        self.assignment_conversions
            .iter()
            .enumerate()
            .map(|(index, node)| (AssignmentId(index as u32), node))
    }
    /// Looks up an owner-local `InitializerId`; out-of-range IDs return `None`.
    pub fn initializer(&self, id: InitializerId) -> Option<&Initializer> {
        self.initializers.get(id.index())
    }
    /// Iterates in arena order, retaining each node's owner-local ID.
    pub fn initializers(
        &self,
    ) -> impl ExactSizeIterator<Item = (InitializerId, &Initializer)> + DoubleEndedIterator {
        self.initializers
            .iter()
            .enumerate()
            .map(|(index, node)| (InitializerId(index as u32), node))
    }
    /// Looks up an owner-local `StatementId`; out-of-range IDs return `None`.
    pub fn statement(&self, id: StatementId) -> Option<&Statement> {
        self.statements.get(id.index())
    }
    /// Iterates in arena order, retaining each node's owner-local ID.
    pub fn statements(
        &self,
    ) -> impl ExactSizeIterator<Item = (StatementId, &Statement)> + DoubleEndedIterator {
        self.statements
            .iter()
            .enumerate()
            .map(|(index, node)| (StatementId(index as u32), node))
    }
    /// Looks up an owner-local `BodyId`; out-of-range IDs return `None`.
    pub fn body(&self, id: BodyId) -> Option<&FunctionBody> {
        self.bodies.get(id.index())
    }
    /// Iterates in arena order, retaining each node's owner-local ID.
    pub fn bodies(
        &self,
    ) -> impl ExactSizeIterator<Item = (BodyId, &FunctionBody)> + DoubleEndedIterator {
        self.bodies
            .iter()
            .enumerate()
            .map(|(index, node)| (BodyId(index as u32), node))
    }
    /// Looks up an owner-local `DeclarationGroupId`; out-of-range IDs return `None`.
    pub fn declaration_group(&self, id: DeclarationGroupId) -> Option<&DeclarationGroup> {
        self.declaration_groups.get(id.index())
    }
    /// Iterates in arena order, retaining each node's owner-local ID.
    pub fn declaration_groups(
        &self,
    ) -> impl ExactSizeIterator<Item = (DeclarationGroupId, &DeclarationGroup)> + DoubleEndedIterator
    {
        self.declaration_groups
            .iter()
            .enumerate()
            .map(|(index, node)| (DeclarationGroupId(index as u32), node))
    }
    /// Looks up an owner-local `AssertionId`; out-of-range IDs return `None`.
    pub fn assertion(&self, id: AssertionId) -> Option<&Assertion> {
        self.assertions.get(id.index())
    }
    /// Iterates in arena order, retaining each node's owner-local ID.
    pub fn assertions(
        &self,
    ) -> impl ExactSizeIterator<Item = (AssertionId, &Assertion)> + DoubleEndedIterator {
        self.assertions
            .iter()
            .enumerate()
            .map(|(index, node)| (AssertionId(index as u32), node))
    }
    /// Written typedef, tag, and member references in checking order.
    pub fn references(&self) -> &[Reference] {
        &self.references
    }
    /// Returns retained `expression_coverage` in source catalog order.
    pub fn expression_coverage(&self) -> &[ExpressionCoverage] {
        &self.expression_coverage
    }
    /// Returns retained `initializer_coverage` in source catalog order.
    pub fn initializer_coverage(&self) -> &[InitializerCoverage] {
        &self.initializer_coverage
    }
    /// Returns retained `statement_coverage` in source catalog order.
    pub fn statement_coverage(&self) -> &[StatementCoverage] {
        &self.statement_coverage
    }
}

impl TypeUse {
    /// Returns the canonical type shape, independent of runtime dimensions.
    pub fn shape(&self) -> TypeId {
        self.shape
    }
    /// Returns runtime bounds and static minimum contracts by structural type path.
    pub fn extents(&self) -> &[Extent] {
        &self.extents
    }
}
impl Extent {
    /// Returns the structural path from the owning type use to this dimension.
    pub fn path(&self) -> &[TypeStep] {
        &self.path
    }
    /// Returns the bound's source and evaluation facts.
    pub fn bound(&self) -> BoundId {
        self.bound
    }
}
impl Bound {
    /// Returns the bound's original preprocessed source span.
    pub fn source(&self) -> &SourceSpan {
        &self.source
    }
    /// Returns the enclosing occurrence, when a derived bound has a source owner.
    pub fn owner(&self) -> Option<OccurrenceId> {
        self.owner
    }
    /// Returns the lexical scope where this bound was checked.
    pub fn scope(&self) -> ScopeId {
        self.scope
    }
    /// Returns the declaration, type-name, or derived bound context.
    pub fn site(&self) -> BoundSite {
        self.site
    }
    /// Whether this dimension specifies a parameter's static minimum length.
    pub fn minimum(&self) -> bool {
        self.minimum
    }
    /// Returns the evaluation requirement; enclosing control flow still applies.
    pub fn evaluation(&self) -> BoundEvaluation {
        self.evaluation
    }
    /// Returns the expression, prototype star, or composed bound inputs.
    pub fn value(&self) -> &BoundValue {
        &self.value
    }
}
impl DeclarationSite {
    /// Returns the final effective type use, including variable bounds.
    pub fn type_use(&self) -> TypeUseId {
        self.type_use
    }
    /// Returns the pre-adjustment parameter type, preserving array contracts.
    pub fn declared_type_use(&self) -> Option<TypeUseId> {
        self.declared_type_use
    }
}
impl Expression {
    /// Returns this expression's type with its runtime dimension links.
    pub fn type_use(&self) -> TypeUseId {
        self.type_use
    }
    /// Returns a written type-name operand's type use, when present.
    pub fn type_name_use(&self) -> Option<TypeUseId> {
        self.type_name_use
    }
}
impl ExprUse {
    /// Returns the effective operand type use after conversions.
    pub fn type_use(&self) -> TypeUseId {
        self.type_use
    }
}
impl CheckedCode {
    /// Looks up an owner-local bound; out-of-range IDs return `None`.
    pub fn bound(&self, id: BoundId) -> Option<&Bound> {
        self.bounds.get(id.index())
    }
    /// Iterates over retained runtime bounds and static parameter contracts.
    pub fn bounds(&self) -> impl ExactSizeIterator<Item = (BoundId, &Bound)> + DoubleEndedIterator {
        self.bounds
            .iter()
            .enumerate()
            .map(|(index, node)| (BoundId(index as u32), node))
    }
    /// Looks up an owner-local type use; out-of-range IDs return `None`.
    pub fn type_use(&self, id: TypeUseId) -> Option<&TypeUse> {
        self.type_uses.get(id.index())
    }
    /// Iterates over types with their structural runtime dimension links.
    pub fn type_uses(
        &self,
    ) -> impl ExactSizeIterator<Item = (TypeUseId, &TypeUse)> + DoubleEndedIterator {
        self.type_uses
            .iter()
            .enumerate()
            .map(|(index, node)| (TypeUseId(index as u32), node))
    }
}

impl TypeUse {
    /// Written function declarators within this type, including nested callbacks.
    /// Multiple entries at one path are distinct possible origins, not a merged
    /// parameter scope. Each parameter site preserves its written array contracts.
    pub fn functions(&self) -> &[FunctionUse] {
        &self.functions
    }
}
impl FunctionUse {
    /// Path from the containing type use to this function shape.
    pub fn path(&self) -> &[TypeStep] {
        &self.path
    }
    /// The written prototype scope, promoted to function scope for a definition.
    pub fn scope(&self) -> ScopeId {
        self.scope
    }
    /// Parameter sites in written order, including unnamed non-void parameters.
    pub fn parameters(&self) -> &[SiteId] {
        &self.parameters
    }
}
impl Occurrence {
    /// Nearest written declaration, parameter, field, function, or type-name owner.
    /// Owners point to themselves; this edge describes syntax, not execution.
    pub fn type_owner(&self) -> Option<OccurrenceId> {
        self.type_owner
    }
    /// GNU `typeof` operands written directly in this owner's type specifiers.
    /// A nested type name owns its own operands. Typedef reuse does not copy them.
    pub fn type_operands(&self) -> &[TypeOperandId] {
        &self.type_operands
    }
}
impl Expression {
    /// The written type-name occurrence for a cast, compound literal, `sizeof`,
    /// `_Alignof`, or `va_arg`. Follow its operands separately from shared bounds.
    pub fn type_name(&self) -> Option<OccurrenceId> {
        self.type_name
    }
}
impl TypeOperand {
    /// The written GNU `typeof` specifier.
    pub fn occurrence(&self) -> OccurrenceId {
        self.occurrence
    }
    /// The declaration, parameter, field, function, or type name that owns this use.
    pub fn owner(&self) -> OccurrenceId {
        self.owner
    }
    /// The lexical scope in which the input was checked.
    pub fn scope(&self) -> ScopeId {
        self.scope
    }
    /// Whether the operand is evaluated when execution reaches its written owner.
    /// Conditional and short-circuit parents still determine whether it is reached.
    pub fn evaluation(&self) -> TypeOperandEvaluation {
        self.evaluation
    }
    /// The checked expression use or written type-name use without array decay.
    pub fn input(&self) -> &TypeOperandInput {
        &self.input
    }
}
impl CheckedCode {
    /// Looks up a GNU `typeof` operand by an ID from this analysis owner.
    pub fn type_operand(&self, id: TypeOperandId) -> Option<&TypeOperand> {
        self.type_operands.get(id.index())
    }
    /// Iterates written GNU `typeof` operands with their owner-local IDs.
    pub fn type_operands(
        &self,
    ) -> impl ExactSizeIterator<Item = (TypeOperandId, &TypeOperand)> + DoubleEndedIterator {
        self.type_operands
            .iter()
            .enumerate()
            .map(|(index, node)| (TypeOperandId(index as u32), node))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_indices_do_not_panic() {
        let analysis = crate::analyze_with_options(
            "int value = 1;",
            toucan_target::Target::X86_64UnknownLinuxGnu,
            &crate::AnalysisOptions {
                retain_code: true,
                ..crate::AnalysisOptions::default()
            },
        )
        .unwrap();
        let code = analysis.checked().unwrap();
        assert!(code.ty(TypeId(u32::MAX)).is_none());
        assert!(code.expression(ExprId(u32::MAX)).is_none());
        assert!(code.initializer(InitializerId(u32::MAX)).is_none());
        assert!(code.statement(StatementId(u32::MAX)).is_none());
        assert!(code.type_use(TypeUseId(u32::MAX)).is_none());
        assert!(code.type_operand(TypeOperandId(u32::MAX)).is_none());
        assert!(code.bound(BoundId(u32::MAX)).is_none());
    }
}
