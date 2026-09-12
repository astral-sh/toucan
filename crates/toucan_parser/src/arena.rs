//! Experimental owned storage for the outer operators of a C expression.
//!
//! Binary, assignment, conditional, and comma operators use indices into a flat
//! vector. Operands parsed by the existing cast-expression grammar remain owned
//! AST leaves, including any operators inside parentheses, casts, or calls.
//! Consequently, this is a partial arena representation, not a fully flat AST.
//!
//! ```
//! use toucan_parser::arena::Expression;
//! use toucan_parser::ast::BinaryOperator;
//! use toucan_parser::driver::{parse_expression_arena, Config};
//!
//! let parsed = parse_expression_arena(&Config::with_gcc(), "a + b * c".into(), |_| false)?;
//! let arena = parsed.expression;
//! let root = arena.get(arena.root()).unwrap();
//! let Expression::Binary { operator, rhs, .. } = &root.node else { panic!("binary root") };
//! assert_eq!(operator.node, BinaryOperator::Plus);
//! let right = arena.get(*rhs).unwrap();
//! assert!(matches!(&right.node,
//!     Expression::Binary { operator, .. } if operator.node == BinaryOperator::Multiply));
//! # Ok::<(), toucan_parser::driver::SyntaxError>(())
//! ```

use ast;
use span::Node;

/// An expression index belonging to one [`ArenaExpression`].
///
/// IDs are stable while their owner lives. Do not mix IDs from different owners:
/// an in-range index from another owner cannot be distinguished during lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExprId(pub(crate) u32);

impl ExprId {
    /// Returns this ID's index in its owner's node vector.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// An expression stored in an arena, with child links local to its owner.
#[derive(Debug)]
pub enum Expression {
    /// A subtree constructed by the existing cast-expression parser.
    /// Its children retain the ordinary owned AST representation.
    Owned(ast::Expression),
    /// A binary operator, including assignments.
    Binary {
        operator: Node<ast::BinaryOperator>,
        lhs: ExprId,
        rhs: ExprId,
    },
    /// A conditional operator, including GNU's omitted middle operand.
    Conditional {
        condition: ExprId,
        then_expression: Option<ExprId>,
        else_expression: ExprId,
    },
    /// A comma expression, in source order.
    Comma(Vec<ExprId>),
}

/// Owns an expression's nodes independently of the input source.
///
/// Children precede their parents in the node vector. No mutable access is
/// exposed, so IDs and the tree structure remain valid. Dropping the arena
/// releases native operators iteratively; owned leaves retain their usual drops.
#[derive(Debug)]
pub struct ArenaExpression {
    nodes: Vec<Node<Expression>>,
    root: ExprId,
}

impl ArenaExpression {
    /// Completes a parser-built tree whose IDs refer to preceding nodes.
    pub(crate) fn new(nodes: Vec<Node<Expression>>, root: ExprId) -> Self {
        Self { nodes, root }
    }

    /// Returns the root expression's ID.
    pub fn root(&self) -> ExprId {
        self.root
    }

    /// Looks up an owner-local ID, returning `None` for an out-of-range index.
    pub fn get(&self, id: ExprId) -> Option<&Node<Expression>> {
        self.nodes.get(id.index())
    }

    /// Iterates over all nodes with children before parents.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (ExprId, &Node<Expression>)> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (ExprId(index as u32), node))
    }

    /// Materializes the ordinary AST for compatibility with existing consumers.
    ///
    /// Conversion moves owned leaves and builds operators without recursion. It
    /// allocates the boxes omitted during arena parsing, plus temporary storage;
    /// include that cost when measuring a consumer that requires the owned AST.
    /// Parsing already checks the resulting owned depth against `max_ast_depth`.
    pub fn into_owned(self) -> Node<ast::Expression> {
        let mut nodes = Vec::with_capacity(self.nodes.len());
        for node in self.nodes {
            let expression = match node.node {
                Expression::Owned(expression) => expression,
                Expression::Binary { operator, lhs, rhs } => {
                    ast::Expression::BinaryOperator(Box::new(Node::new(
                        ast::BinaryOperatorExpression {
                            operator,
                            lhs: Box::new(take(&mut nodes, lhs)),
                            rhs: Box::new(take(&mut nodes, rhs)),
                        },
                        node.span,
                    )))
                }
                Expression::Conditional {
                    condition,
                    then_expression,
                    else_expression,
                } => ast::Expression::Conditional(Box::new(Node::new(
                    ast::ConditionalExpression {
                        condition: Box::new(take(&mut nodes, condition)),
                        then_expression: then_expression.map(|id| Box::new(take(&mut nodes, id))),
                        else_expression: Box::new(take(&mut nodes, else_expression)),
                    },
                    node.span,
                ))),
                Expression::Comma(expressions) => ast::Expression::Comma(Box::new(
                    expressions
                        .into_iter()
                        .map(|id| take(&mut nodes, id))
                        .collect(),
                )),
            };
            nodes.push(Some(Node::new(expression, node.span)));
        }
        take(&mut nodes, self.root)
    }
}

/// Moves a child from the parser-built tree exactly once.
fn take(nodes: &mut [Option<Node<ast::Expression>>], id: ExprId) -> Node<ast::Expression> {
    nodes[id.index()]
        .take()
        .expect("arena child precedes its unique parent")
}
