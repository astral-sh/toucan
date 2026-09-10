use toucan_semantic::{DeclarationKind, IntegerKind, TypeKind, analyze, evaluate_integer};
use toucan_target::Target;

const TARGET: Target = Target::X86_64UnknownLinuxGnu;

#[test]
fn resolves_recursive_records_typedefs_and_callback_parameters() {
    let unit = analyze(
        r#"
        typedef struct Node Node;
        typedef int (*visit_cb)(const Node *, void *);
        struct Node { const char *name; Node *next; visit_cb visit; };
        extern int walk(Node *, visit_cb cb, void *context);
    "#,
        TARGET,
    )
    .unwrap();
    let node = &unit.typedefs["Node"];
    let layout = unit.layout(node).unwrap();
    assert_eq!((layout.size_bytes(), layout.alignment_bytes()), (24, 8));
    let callback = unit.resolve(&unit.typedefs["visit_cb"]).unwrap();
    let TypeKind::Pointer(function) = &callback.kind else {
        panic!("callback pointer")
    };
    let TypeKind::Function(function) = &function.kind else {
        panic!("callback function")
    };
    let TypeKind::Pointer(parameter) = &function.parameters[0].ty.kind else {
        panic!("node pointer")
    };
    assert!(parameter.qualifiers.is_const);
    assert_eq!(
        unit.declarations.last().unwrap().kind,
        DeclarationKind::Function
    );
}

#[test]
fn distinguishes_declarator_binding_and_parameter_decay() {
    let unit = analyze(
        "int *matrix[3][4]; int (*row)[4]; int *make(void); void consume(int a[7], int f(int));",
        TARGET,
    )
    .unwrap();
    assert_eq!(
        unit.layout(&unit.declarations[0].ty).unwrap().size_bytes(),
        96
    );
    let TypeKind::Pointer(row) = &unit.declarations[1].ty.kind else {
        panic!("row pointer")
    };
    assert!(matches!(
        row.kind,
        TypeKind::Array {
            length: Some(4),
            ..
        }
    ));
    let TypeKind::Function(make) = &unit.declarations[2].ty.kind else {
        panic!("make function")
    };
    assert!(matches!(make.return_type.kind, TypeKind::Pointer(_)));
    assert!(make.prototype && make.parameters.is_empty());
    let TypeKind::Function(consume) = &unit.declarations[3].ty.kind else {
        panic!("consume function")
    };
    assert!(
        consume
            .parameters
            .iter()
            .all(|parameter| matches!(parameter.ty.kind, TypeKind::Pointer(_)))
    );
}

#[test]
fn integer_promotions_conversions_short_circuit_and_ternary() {
    let unit = analyze(
        "typedef unsigned char byte; enum { A = 3, B = A * 7, C };",
        TARGET,
    )
    .unwrap();
    for (expression, expected) in [
        ("B + C", 43),
        ("-1 < 1U", 0),
        ("(byte)255 + 1", 256),
        ("0 && (1 / 0)", 0),
        ("1 || (1 / 0)", 1),
        ("1 ? 4 : 1 / 0", 4),
        ("(1 ? -1 : 1U) < 0", 0),
        ("~0U + 1U", 0),
        ("-7 / 3", -2),
        ("-7 % 3", -1),
        ("'\\n'", 10),
        ("'A' + 1", 66),
        ("sizeof(byte)", 1),
    ] {
        assert_eq!(
            evaluate_integer(&unit, expression).unwrap().signed_value(),
            expected,
            "{expression}"
        );
    }
    let mask = evaluate_integer(&unit, "0xffffffff").unwrap();
    assert!(!mask.signed);
    assert_eq!(mask.bits, 32);
}

#[test]
fn target_integer_widths_change_constants() {
    let linux = analyze("", TARGET).unwrap();
    let windows = analyze("", Target::X86_64PcWindowsMsvc).unwrap();
    assert_eq!(evaluate_integer(&linux, "sizeof(long)").unwrap().value, 8);
    assert_eq!(evaluate_integer(&windows, "sizeof(long)").unwrap().value, 4);
    assert_eq!(evaluate_integer(&linux, "-1L < 1U").unwrap().value, 1);
    assert_eq!(evaluate_integer(&windows, "-1L < 1U").unwrap().value, 0);
}

#[test]
fn undefined_integer_expressions_are_errors() {
    let unit = analyze("", TARGET).unwrap();
    for expression in [
        "1 / 0",
        "1 << 32",
        "1 << -1",
        "2147483647 + 1",
        "-2147483647 - 2",
        "(-2147483647 - 1) / -1",
        "(-2147483647 - 1) % -1",
        "-1 << 2",
    ] {
        assert!(evaluate_integer(&unit, expression).is_err(), "{expression}");
    }
}

#[test]
fn packing_and_alignment_survive_preprocessing() {
    let unit = analyze(
        r#"
        #pragma pack(push, wire, 1)
        struct Wire { char tag; int value; };
        #pragma pack(pop, wire)
        struct Native { char tag; int value; };
        struct __attribute__((packed, aligned(8))) Aligned { char tag; int value; };
    "#,
        TARGET,
    )
    .unwrap();
    let record = |name| {
        toucan_semantic::Type::new(TypeKind::Record(
            unit.records
                .iter()
                .position(|record| record.name.as_deref() == Some(name))
                .unwrap(),
        ))
    };
    assert_eq!(unit.layout(&record("Wire")).unwrap().size_bytes(), 5);
    assert_eq!(unit.layout(&record("Native")).unwrap().size_bytes(), 8);
    assert_eq!(
        unit.layout(&record("Aligned")).unwrap().alignment_bytes(),
        8
    );
}

#[test]
fn bitfields_flexible_arrays_and_offsetof() {
    let unit = analyze(
        r#"
        struct Flags { unsigned int first:3; unsigned int:0; unsigned int second:5; };
        struct Packet { int tag; char bytes[]; };
        struct Nested { char tag; struct { int values[4]; } inner; };
        enum { OFFSET = __builtin_offsetof(struct Nested, inner.values[2]) };
        _Static_assert(sizeof(struct Flags) == 8, "flags");
    "#,
        TARGET,
    )
    .unwrap();
    assert_eq!(unit.constants["OFFSET"].value, 12);
    assert!(evaluate_integer(&unit, "__builtin_offsetof(struct Flags, first)").is_err());
    assert!(analyze("struct Bad { int values[]; int after; };", TARGET).is_err());
}

#[test]
fn keeps_link_names_and_marks_definitions() {
    let unit = analyze(
        r#"
        extern int api(int) __asm__("real_api") __attribute__((nothrow));
        static inline int helper(int x) { return x + 1; }
        extern int variadic(const char *, ...);
        extern int legacy();
    "#,
        TARGET,
    )
    .unwrap();
    assert_eq!(unit.declarations[0].link_name.as_deref(), Some("real_api"));
    let labeled = serde_json::to_value(&unit.declarations[0]).unwrap();
    assert_eq!(labeled["link_name_is_literal"], true);
    let ordinary = serde_json::to_value(&unit.declarations[1]).unwrap();
    assert!(ordinary.get("link_name_is_literal").is_none());
    assert!(unit.declarations[1].is_static && unit.declarations[1].is_definition);
    let TypeKind::Function(variadic) = &unit.declarations[2].ty.kind else {
        panic!("variadic")
    };
    assert!(variadic.variadic);
    let TypeKind::Function(legacy) = &unit.declarations[3].ty.kind else {
        panic!("legacy")
    };
    assert!(!legacy.prototype);
}

#[test]
fn rejects_conflicts_and_unknown_abi_features() {
    for source in [
        "typedef int T; typedef double T;",
        "struct S { int a; int a; };",
        "int f(int); double f(int);",
        "struct S; struct S { struct S value; };",
        "struct S { int named:0; };",
        "struct S { int too_wide:33; };",
        "typedef int v8 __attribute__((vector_size(32)));",
        "void f(void) __attribute__((vectorcall));",
        "_Atomic(const int) atomic_value;",
        "_Static_assert(0, \"failure\");",
    ] {
        assert!(analyze(source, TARGET).is_err(), "{source}");
    }
}

#[test]
fn repeated_compatible_prototypes_ignore_parameter_names() {
    let unit = analyze("int call(int a); int call(int b);", TARGET).unwrap();
    assert_eq!(unit.declarations.len(), 1);
    let TypeKind::Function(function) = &unit.declarations[0].ty.kind else {
        panic!("function")
    };
    assert!(matches!(
        function.parameters[0].ty.kind,
        TypeKind::Integer(IntegerKind::Int)
    ));
}

#[test]
fn malformed_deep_input_is_rejected_before_recursive_parsing() {
    let source = format!("int x = {}1{};", "(".repeat(10000), ")".repeat(10000));
    assert!(
        analyze(&source, TARGET)
            .unwrap_err()
            .message
            .contains("limit")
    );
    let source = format!("int {}x;", "*".repeat(10000));
    assert!(
        analyze(&source, TARGET)
            .unwrap_err()
            .message
            .contains("limit")
    );
}

#[test]
fn integer_machine_modes_preserve_width_and_signedness() {
    let source = r#"
        typedef int register_t __attribute__ ( ( __mode__ ( __word__ ) ) );
        typedef unsigned int byte __attribute__((mode(QI)));
        typedef int half __attribute__((mode(HI)));
        typedef unsigned int wide __attribute__((mode(TI)));
    "#;
    let unit = analyze(source, TARGET).unwrap();
    assert_eq!(
        evaluate_integer(&unit, "sizeof(register_t)").unwrap().value,
        8
    );
    assert_eq!(evaluate_integer(&unit, "sizeof(byte)").unwrap().value, 1);
    assert_eq!(evaluate_integer(&unit, "sizeof(half)").unwrap().value, 2);
    assert_eq!(evaluate_integer(&unit, "sizeof(wide)").unwrap().value, 16);
    assert_eq!(evaluate_integer(&unit, "(byte)-1").unwrap().value, 255);
    assert_eq!(
        evaluate_integer(&unit, "(wide)1 << 127").unwrap().value,
        1u128 << 127
    );
    assert!(analyze("typedef float F __attribute__((mode(DF)));", TARGET).is_ok());
    assert!(analyze("typedef float F __attribute__((mode(QI)));", TARGET).is_err());
    let windows = analyze(
        "typedef int register_t __attribute__((mode(word))); typedef int wide __attribute__((mode(TI)));",
        Target::X86_64PcWindowsMsvc,
    )
    .unwrap();
    assert_eq!(
        evaluate_integer(&windows, "sizeof(wide)").unwrap().value,
        16
    );
    assert_eq!(
        evaluate_integer(&windows, "sizeof(register_t)")
            .unwrap()
            .value,
        8
    );
}

#[test]
fn clang_glibc_float_typedefs_are_preserved() {
    let analysis = toucan_semantic::analyze_with_profile("typedef float _Float32; typedef double _Float64; typedef double _Float32x; typedef long double _Float64x; typedef __float128 _Float128; extern _Float64 strtof64(const char *); extern _Float128 future(void);", toucan_target::CompilerProfile::new(TARGET, toucan_target::Compiler::Clang).unwrap(), &Default::default()).unwrap();
    let unit = analysis.unit();
    assert_eq!(evaluate_integer(unit, "sizeof(_Float32)").unwrap().value, 4);
    assert_eq!(evaluate_integer(unit, "sizeof(_Float64)").unwrap().value, 8);
    let TypeKind::Function(function) = &unit.declarations.last().unwrap().ty.kind else {
        panic!("function")
    };
    assert!(matches!(
        unit.resolve(&function.return_type).unwrap().kind,
        TypeKind::Float(toucan_semantic::FloatKind::Extended { width: 128, .. })
    ));
    assert_eq!(unit.layout(&function.return_type).unwrap().size_bits, 128);
}

#[test]
fn invalid_public_ir_produces_layout_errors() {
    let unit = analyze("", TARGET).unwrap();
    assert!(
        unit.layout(&toucan_semantic::Type::new(TypeKind::Record(9999)))
            .is_err()
    );
    assert!(
        unit.layout(&toucan_semantic::Type::new(TypeKind::Enum(9999)))
            .is_err()
    );
    let mut ty = toucan_semantic::Type::new(TypeKind::Integer(IntegerKind::Int));
    for _ in 0..256 {
        ty = toucan_semantic::Type::new(TypeKind::Array {
            element: Box::new(ty),
            length: Some(1),
        });
    }
    assert!(unit.layout(&ty).is_err());
}

#[test]
fn redeclarations_preserve_qualifiers_and_complete_types() {
    assert!(analyze("extern const int *p; extern int *p;", TARGET).is_err());
    assert!(analyze("int f(); int f(float value);", TARGET).is_err());
    assert!(analyze("int f(); int f(int value, ...);", TARGET).is_err());
    let unit = analyze("typedef const int CI; int f(const int a, CI *ptr); int f(int a, const int *ptr); int old(); int old(int); extern int values[]; extern int values[3];", TARGET).unwrap();
    let old = unit
        .declarations
        .iter()
        .find(|declaration| declaration.name == "old")
        .unwrap();
    let TypeKind::Function(function) = &old.ty.kind else {
        panic!("function")
    };
    assert!(function.prototype);
    let array = unit
        .declarations
        .iter()
        .find(|declaration| declaration.name == "values")
        .unwrap();
    assert_eq!(unit.layout(&array.ty).unwrap().size_bytes(), 12);
}

#[test]
fn enumerators_use_int_when_representable() {
    let unit = analyze("enum E { A=1U, B=2L };", TARGET).unwrap();
    assert_eq!(evaluate_integer(&unit, "(A-2)<0").unwrap().value, 1);
    assert_eq!(evaluate_integer(&unit, "sizeof(B)").unwrap().value, 4);
}

#[test]
fn malformed_prefix_runs_do_not_trigger_parser_backtracking() {
    let source = format!("signed long size[{}$];", "+".repeat(32));
    assert!(
        analyze(&source, TARGET)
            .unwrap_err()
            .message
            .contains("parser BacktrackingSteps limit")
    );
}

#[test]
fn macro_expressions_cannot_escape_the_parse_wrapper() {
    let unit = analyze("", TARGET).unwrap();
    for expression in [
        "0); __typeof__(int",
        "0); int x=(2",
        "0), y=(2",
        "0) //",
        "0); int __toucan_expression=(2",
    ] {
        assert!(evaluate_integer(&unit, expression).is_err(), "{expression}");
    }
    assert_eq!(evaluate_integer(&unit, "1 /* ; ) */ + 2").unwrap().value, 3);
}

#[test]
fn macro_expressions_resolve_referenced_typedefs_and_their_dependencies() {
    let mut unit = analyze(
        "typedef unsigned char Byte; typedef Byte Bytes[3]; typedef Bytes *BytesPointer; enum { ByteCount = 11 };",
        TARGET,
    )
    .unwrap();
    for (expression, expected) in [
        ("(Byte)255 + 1", 256),
        ("sizeof(Bytes)", 3),
        ("sizeof(BytesPointer)", 8),
        ("sizeof((BytesPointer)0)", 8),
        ("sizeof(/* Byte */ Bytes /* BytesPointer */)", 3),
        ("sizeof(\"Byte BytesPointer\")", 18),
        ("ByteCount + 'B' + 0xBU", 88),
    ] {
        assert_eq!(evaluate_integer(&unit, expression).unwrap().value, expected);
    }
    let expression = "sizeof(Bytes) + missing";
    assert_eq!(
        evaluate_integer(&unit, expression).unwrap_err().offset,
        expression.find("missing").unwrap()
    );

    // Public IR can be mutated; even unrelated malformed names remain errors.
    unit.typedefs
        .insert("bad;name".into(), unit.typedefs["Byte"].clone());
    assert!(evaluate_integer(&unit, "1").is_err());
}

#[test]
fn macro_syntax_diagnostics_are_relative_to_the_expression() {
    let empty = analyze("", TARGET).unwrap();
    let aliases = analyze("typedef char Byte; typedef int Count;", TARGET).unwrap();
    for expression in ["extern", "1 +\nextern"] {
        let without_aliases = evaluate_integer(&empty, expression).unwrap_err();
        let with_aliases = evaluate_integer(&aliases, expression).unwrap_err();
        assert_eq!(without_aliases.message, with_aliases.message);
        assert_eq!(without_aliases.offset, with_aliases.offset);
    }
    let error = evaluate_integer(&aliases, "sizeof(Byte) +\nextern").unwrap_err();
    assert!(error.message.contains("line 2 column 7"), "{error}");
    assert_eq!(error.offset, 21);
}

#[test]
fn array_typedef_parameter_decay_preserves_element_qualifiers() {
    let unit = analyze("typedef int Array[4]; typedef const Array ConstArray; void consume(const Array argument); void consume_alias(volatile ConstArray argument); typedef int *Pointers[4]; void pointer_array(const Pointers argument);", TARGET).unwrap();
    for name in ["consume", "consume_alias", "pointer_array"] {
        let declaration = unit
            .declarations
            .iter()
            .find(|declaration| declaration.name == name)
            .unwrap();
        let TypeKind::Function(function) = &declaration.ty.kind else {
            panic!("function")
        };
        let TypeKind::Pointer(element) = &function.parameters[0].ty.kind else {
            panic!("adjusted array parameter")
        };
        assert!(unit.qualifiers(element).unwrap().is_const, "{name}");
        if name == "consume_alias" {
            assert!(unit.qualifiers(element).unwrap().is_volatile);
        }
        if name == "pointer_array" {
            let TypeKind::Pointer(pointee) = &element.kind else {
                panic!("pointer element")
            };
            assert!(!pointee.qualifiers.is_const);
        }
    }
}

#[test]
fn msvc_enum_casts_use_the_signed_int_representation() {
    let source = "enum E { ZERO=0 }; _Static_assert((enum E)-1<0, \"signed\");";
    let unit = analyze(source, Target::X86_64PcWindowsMsvc).unwrap();
    assert_eq!(
        evaluate_integer(&unit, "(enum E)-1")
            .unwrap()
            .signed_value(),
        -1
    );
    assert_eq!(evaluate_integer(&unit, "sizeof(enum E)").unwrap().value, 4);
    let unix = analyze("enum E { ZERO=0 };", TARGET).unwrap();
    assert_eq!(evaluate_integer(&unix, "(enum E)-1<0").unwrap().value, 0);
}

#[test]
fn sizeof_validates_unevaluated_names_and_scalar_operands() {
    for expression in [
        "missing == unknown",
        "missing && unknown",
        "!missing",
        "+missing",
        "(int)missing",
        "sizeof(missing)",
        "missing ? 1 : 2",
    ] {
        let source = format!("_Static_assert(sizeof({expression}) == 4, \"bad\");");
        assert!(analyze(&source, TARGET).is_err(), "{expression}");
    }
    assert!(
        analyze(
            "struct S { int a; }; extern struct S s; _Static_assert(sizeof(!s)==4, \"bad\");",
            TARGET
        )
        .is_err()
    );
    analyze("_Static_assert(sizeof(1 / 0)==4, \"unevaluated division\"); _Static_assert(sizeof((1 / 0) == 0)==4, \"unevaluated comparison\"); _Static_assert(sizeof(!(1 / 0))==4, \"unevaluated unary\");", TARGET).unwrap();
}

#[test]
fn object_alignment_does_not_change_record_layout() {
    for declaration in [
        "struct S __attribute__((aligned(16))) object;",
        "__attribute__((aligned(16))) struct S object;",
        "struct S object __attribute__((aligned(16)));",
        "_Alignas(16) struct S object;",
    ] {
        analyze(
            &format!(
                "struct S {{ char x; }}; {declaration}
                 struct Wrapper {{ struct S s; char after; }};
                 _Static_assert(sizeof(struct S) == 1, \"record size\");
                 _Static_assert(_Alignof(struct S) == 1, \"record alignment\");
                 _Static_assert(sizeof(struct Wrapper) == 2, \"containing record\");"
            ),
            TARGET,
        )
        .unwrap();
    }
    for declaration in [
        "struct __attribute__((aligned(16))) S { char x; } object;",
        "struct S { char x; } __attribute__((aligned(16))) object;",
    ] {
        analyze(
            &format!(
                "{declaration}
                 _Static_assert(sizeof(struct S) == 16, \"record size\");
                 _Static_assert(_Alignof(struct S) == 16, \"record alignment\");"
            ),
            TARGET,
        )
        .unwrap();
    }
    analyze(
        "__attribute__((aligned(16))) struct S { char x; } object;
         _Static_assert(sizeof(struct S) == 1, \"object attribute before definition\");
         struct Outer { struct __attribute__((packed)) Inner { char x; int y; } inner; char z; };
         _Static_assert(sizeof(struct Inner) == 5, \"nested record attribute\");
         _Static_assert(sizeof(struct Outer) == 6, \"containing nested record\");",
        TARGET,
    )
    .unwrap();
}

#[test]
fn forward_record_attributes_follow_target_compiler() {
    for (target, size) in [(TARGET, 1), (Target::Aarch64AppleDarwin, 16)] {
        analyze(
            &format!(
                "struct __attribute__((aligned(16))) S; struct S {{ char x; }};
                 _Static_assert(sizeof(struct S) == {size}, \"forward tag alignment\");"
            ),
            target,
        )
        .unwrap();
        analyze(
            "struct S { char x; }; struct __attribute__((aligned(16))) S object;
             _Static_assert(sizeof(struct S) == 1, \"attribute after definition is ignored\");",
            target,
        )
        .unwrap();
    }
    analyze("enum __attribute__((aligned(16))) E { A }; _Static_assert(_Alignof(enum E)==4,\"GNU ignores enum tag alignment\");", TARGET).unwrap();
}

#[test]
fn short_circuit_operands_are_type_checked_without_evaluation() {
    let unit = analyze("struct S { int x; }; struct S object;", TARGET).unwrap();
    for expression in ["1 || unknown", "0 && unknown", "1 || object", "0 && object"] {
        assert!(evaluate_integer(&unit, expression).is_err(), "{expression}");
    }
    for (expression, expected) in [("1 || (1 / 0)", 1), ("0 && (1 / 0)", 0)] {
        assert_eq!(evaluate_integer(&unit, expression).unwrap().value, expected);
    }
}
