// GCC 13 / Clang 18 observations. The native oracle in alignof_expression.rs
// checks these cases against installed compilers; historical fixture:
// https://github.com/astral-sh/toucan/blob/8b6681044c309f8b85abf5eb96ed2a72e538a441/crates/toucan_semantic/tests/fixtures/alignof_queries.rs
const QUERY_PROFILES: [(Target, Compiler); 7] = [
    (Target::X86_64UnknownLinuxGnu, Compiler::Gnu),
    (Target::Aarch64UnknownLinuxGnu, Compiler::Gnu),
    (Target::X86_64UnknownLinuxGnu, Compiler::Clang),
    (Target::Aarch64UnknownLinuxGnu, Compiler::Clang),
    (Target::X86_64AppleDarwin, Compiler::Clang),
    (Target::Aarch64AppleDarwin, Compiler::Clang),
    (Target::X86_64PcWindowsMsvc, Compiler::Clang),
];

const OBSERVATIONS: &[(&str, &str, [Option<u64>; 7])] = &[
    ("evidence.json:object:__alignof__", r###"int object;
unsigned long long result=__alignof__(object);
"###, [Some(4); 7]),
    ("evidence.json:object:_Alignof", r###"int object;
unsigned long long result=_Alignof(object);
"###, [Some(4); 7]),
    ("evidence.json:aligned-object:__alignof__", r###"int object __attribute__((aligned(16)));
unsigned long long result=__alignof__(object);
"###, [Some(16); 7]),
    ("evidence.json:aligned-object:_Alignof", r###"int object __attribute__((aligned(16)));
unsigned long long result=_Alignof(object);
"###, [Some(16); 7]),
    ("evidence.json:lowered-object:__alignof__", r###"int object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object);
"###, [Some(1); 7]),
    ("evidence.json:lowered-object:_Alignof", r###"int object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object);
"###, [Some(1); 7]),
    ("evidence.json:alignas-object:__alignof__", r###"_Alignas(32) int object;
unsigned long long result=__alignof__(object);
"###, [Some(32); 7]),
    ("evidence.json:alignas-object:_Alignof", r###"_Alignas(32) int object;
unsigned long long result=_Alignof(object);
"###, [Some(32); 7]),
    ("evidence.json:aligned-typedef:__alignof__", r###"typedef int T __attribute__((aligned(16)));T object;
unsigned long long result=__alignof__(object);
"###, [Some(16); 7]),
    ("evidence.json:aligned-typedef:_Alignof", r###"typedef int T __attribute__((aligned(16)));T object;
unsigned long long result=_Alignof(object);
"###, [Some(16); 7]),
    ("evidence.json:packed-field:__alignof__", r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=__alignof__(object.value);
"###, [Some(1); 7]),
    ("evidence.json:packed-field:_Alignof", r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=_Alignof(object.value);
"###, [Some(1); 7]),
    ("evidence.json:aligned-field:__alignof__", r###"struct S{char lead;int value __attribute__((aligned(16)));};struct S object;
unsigned long long result=__alignof__(object.value);
"###, [Some(16); 7]),
    ("evidence.json:aligned-field:_Alignof", r###"struct S{char lead;int value __attribute__((aligned(16)));};struct S object;
unsigned long long result=_Alignof(object.value);
"###, [Some(16); 7]),
    ("evidence.json:packed-aligned-field:__alignof__", r###"struct __attribute__((packed)) S{char lead;int value __attribute__((aligned(8)));};struct S object;
unsigned long long result=__alignof__(object.value);
"###, [Some(8); 7]),
    ("evidence.json:packed-aligned-field:_Alignof", r###"struct __attribute__((packed)) S{char lead;int value __attribute__((aligned(8)));};struct S object;
unsigned long long result=_Alignof(object.value);
"###, [Some(8); 7]),
    ("evidence.json:nested-packed:__alignof__", r###"struct Inner{char lead;int value;};struct __attribute__((packed)) Outer{char lead;struct Inner inner;};struct Outer object;
unsigned long long result=__alignof__(object.inner.value);
"###, [Some(4); 7]),
    ("evidence.json:nested-packed:_Alignof", r###"struct Inner{char lead;int value;};struct __attribute__((packed)) Outer{char lead;struct Inner inner;};struct Outer object;
unsigned long long result=_Alignof(object.inner.value);
"###, [Some(4); 7]),
    ("evidence.json:aligned-container:__alignof__", r###"struct S{int value;};struct S object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object.value);
"###, [Some(4); 7]),
    ("evidence.json:aligned-container:_Alignof", r###"struct S{int value;};struct S object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object.value);
"###, [Some(4); 7]),
    ("evidence.json:packed-address:__alignof__", r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=__alignof__(*(&object.value));
"###, [Some(1), Some(1), Some(4), Some(4), Some(4), Some(4), Some(4)]),
    ("evidence.json:packed-address:_Alignof", r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=_Alignof(*(&object.value));
"###, [Some(1), Some(1), Some(4), Some(4), Some(4), Some(4), Some(4)]),
    ("evidence.json:array:__alignof__", r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, [Some(32); 7]),
    ("evidence.json:array:_Alignof", r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, [Some(32); 7]),
    ("evidence.json:element-zero:__alignof__", r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object[0]);
"###, [Some(4); 7]),
    ("evidence.json:element-zero:_Alignof", r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object[0]);
"###, [Some(4); 7]),
    ("evidence.json:element-one:__alignof__", r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object[1]);
"###, [Some(4); 7]),
    ("evidence.json:element-one:_Alignof", r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object[1]);
"###, [Some(4); 7]),
    ("evidence.json:element-variable:__alignof__", r###"int object[4] __attribute__((aligned(32)));int i;
unsigned long long result=__alignof__(object[i]);
"###, [Some(4); 7]),
    ("evidence.json:element-variable:_Alignof", r###"int object[4] __attribute__((aligned(32)));int i;
unsigned long long result=_Alignof(object[i]);
"###, [Some(4); 7]),
    ("evidence.json:dereference:__alignof__", r###"int *object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*object);
"###, [Some(4); 7]),
    ("evidence.json:dereference:_Alignof", r###"int *object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*object);
"###, [Some(4); 7]),
    ("evidence.json:aligned-pointee:__alignof__", r###"typedef int T __attribute__((aligned(16)));T *object;
unsigned long long result=__alignof__(*object);
"###, [Some(16); 7]),
    ("evidence.json:aligned-pointee:_Alignof", r###"typedef int T __attribute__((aligned(16)));T *object;
unsigned long long result=_Alignof(*object);
"###, [Some(16); 7]),
    ("evidence.json:cast:__alignof__", r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=__alignof__((T)0);
"###, [Some(4), Some(4), Some(16), Some(16), Some(16), Some(16), Some(16)]),
    ("evidence.json:cast:_Alignof", r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=_Alignof((T)0);
"###, [Some(4), Some(4), Some(16), Some(16), Some(16), Some(16), Some(16)]),
    ("evidence.json:compound-literal:__alignof__", r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=__alignof__((T){0});
"###, [Some(16); 7]),
    ("evidence.json:compound-literal:_Alignof", r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=_Alignof((T){0});
"###, [Some(16); 7]),
    ("evidence.json:conditional:__alignof__", r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=__alignof__(1?object:plain);
"###, [Some(4); 7]),
    ("evidence.json:conditional:_Alignof", r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=_Alignof(1?object:plain);
"###, [Some(4); 7]),
    ("evidence.json:comma:__alignof__", r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(0,object);
"###, [Some(4); 7]),
    ("evidence.json:comma:_Alignof", r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(0,object);
"###, [Some(4); 7]),
    ("evidence.json:generic:__alignof__", r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(_Generic(0,int:object,default:1));
"###, [Some(32); 7]),
    ("evidence.json:generic:_Alignof", r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(_Generic(0,int:object,default:1));
"###, [Some(32); 7]),
    ("evidence.json:literal:__alignof__", r###"
unsigned long long result=__alignof__(3);
"###, [Some(4); 7]),
    ("evidence.json:literal:_Alignof", r###"
unsigned long long result=_Alignof(3);
"###, [Some(4); 7]),
    ("evidence.json:string:__alignof__", r###"
unsigned long long result=__alignof__("abc");
"###, [Some(1); 7]),
    ("evidence.json:string:_Alignof", r###"
unsigned long long result=_Alignof("abc");
"###, [Some(1); 7]),
    ("evidence.json:function:__alignof__", r###"void object(void);
unsigned long long result=__alignof__(object);
"###, [Some(1), Some(4), Some(4), Some(4), Some(4), Some(4), Some(4)]),
    ("evidence.json:function:_Alignof", r###"void object(void);
unsigned long long result=_Alignof(object);
"###, [Some(1), Some(4), Some(4), Some(4), Some(4), Some(4), Some(4)]),
    ("evidence.json:aligned-function:__alignof__", r###"void object(void) __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, [Some(32); 7]),
    ("evidence.json:aligned-function:_Alignof", r###"void object(void) __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, [Some(32); 7]),
    ("evidence.json:function-pointer:__alignof__", r###"void (*object)(void);
unsigned long long result=__alignof__(*object);
"###, [Some(1), Some(4), Some(4), Some(4), Some(4), Some(4), Some(4)]),
    ("evidence.json:function-pointer:_Alignof", r###"void (*object)(void);
unsigned long long result=_Alignof(*object);
"###, [Some(1), Some(4), Some(4), Some(4), Some(4), Some(4), Some(4)]),
    ("evidence.json:function-call:__alignof__", r###"int object(void);
unsigned long long result=__alignof__(object());
"###, [Some(4); 7]),
    ("evidence.json:function-call:_Alignof", r###"int object(void);
unsigned long long result=_Alignof(object());
"###, [Some(4); 7]),
    ("evidence.json:void:__alignof__", r###"
unsigned long long result=__alignof__((void)0);
"###, [Some(1); 7]),
    ("evidence.json:void:_Alignof", r###"
unsigned long long result=_Alignof((void)0);
"###, [Some(1); 7]),
    ("evidence.json:incomplete:__alignof__", r###"struct S;extern struct S object;
unsigned long long result=__alignof__(object);
"###, [Some(1), Some(1), None, None, None, None, None]),
    ("evidence.json:incomplete:_Alignof", r###"struct S;extern struct S object;
unsigned long long result=_Alignof(object);
"###, [Some(1), Some(1), None, None, None, None, None]),
    ("evidence.json:bitfield:__alignof__", r###"struct S{unsigned value:3;};struct S object;
unsigned long long result=__alignof__(object.value);
"###, [None; 7]),
    ("evidence.json:bitfield:_Alignof", r###"struct S{unsigned value:3;};struct S object;
unsigned long long result=_Alignof(object.value);
"###, [None; 7]),
    ("evidence.json:vector:__alignof__", r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=__alignof__(object);
"###, [Some(16); 7]),
    ("evidence.json:vector:_Alignof", r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=_Alignof(object);
"###, [Some(16); 7]),
    ("evidence.json:vector-lane:__alignof__", r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=__alignof__(object[0]);
"###, [Some(4); 7]),
    ("evidence.json:vector-lane:_Alignof", r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=_Alignof(object[0]);
"###, [Some(4); 7]),
    ("evidence.json:lowered-vector:__alignof__", r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object);
"###, [Some(1); 7]),
    ("evidence.json:lowered-vector:_Alignof", r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object);
"###, [Some(1); 7]),
    ("evidence.json:lowered-vector-lane:__alignof__", r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object[0]);
"###, [Some(4); 7]),
    ("evidence.json:lowered-vector-lane:_Alignof", r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object[0]);
"###, [Some(4); 7]),
    ("extra/evidence.json:packed-plus:__alignof__", r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=__alignof__(*(&object.v+0));
"###, [Some(1), Some(1), Some(4), Some(4), Some(4), Some(4), Some(4)]),
    ("extra/evidence.json:packed-plus:_Alignof", r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=_Alignof(*(&object.v+0));
"###, [Some(1), Some(1), Some(4), Some(4), Some(4), Some(4), Some(4)]),
    ("extra/evidence.json:aligned-roundtrip:__alignof__", r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object));
"###, [Some(32), Some(32), Some(4), Some(4), Some(4), Some(4), Some(4)]),
    ("extra/evidence.json:aligned-roundtrip:_Alignof", r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object));
"###, [Some(32), Some(32), Some(4), Some(4), Some(4), Some(4), Some(4)]),
    ("extra/evidence.json:aligned-plus:__alignof__", r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+0));
"###, [Some(32), Some(32), Some(4), Some(4), Some(4), Some(4), Some(4)]),
    ("extra/evidence.json:aligned-plus:_Alignof", r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+0));
"###, [Some(32), Some(32), Some(4), Some(4), Some(4), Some(4), Some(4)]),
    ("extra/evidence.json:packed-cast:__alignof__", r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=__alignof__(*((int*)&object.v));
"###, [Some(1), Some(1), Some(4), Some(4), Some(4), Some(4), Some(4)]),
    ("extra/evidence.json:packed-cast:_Alignof", r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=_Alignof(*((int*)&object.v));
"###, [Some(1), Some(1), Some(4), Some(4), Some(4), Some(4), Some(4)]),
    ("extra/evidence.json:aligned-conditional:__alignof__", r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=__alignof__(*(1?&object:&plain));
"###, [Some(4); 7]),
    ("extra/evidence.json:aligned-conditional:_Alignof", r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=_Alignof(*(1?&object:&plain));
"###, [Some(4); 7]),
    ("extra/evidence.json:redecl-increase:__alignof__", r###"extern int object __attribute__((aligned(8)));extern int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, [Some(32); 7]),
    ("extra/evidence.json:redecl-increase:_Alignof", r###"extern int object __attribute__((aligned(8)));extern int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, [Some(32); 7]),
    ("extra/evidence.json:redecl-decrease:__alignof__", r###"extern int object __attribute__((aligned(32)));extern int object __attribute__((aligned(8)));
unsigned long long result=__alignof__(object);
"###, [Some(32); 7]),
    ("extra/evidence.json:redecl-decrease:_Alignof", r###"extern int object __attribute__((aligned(32)));extern int object __attribute__((aligned(8)));
unsigned long long result=_Alignof(object);
"###, [Some(32); 7]),
    ("extra/evidence.json:redecl-plain:__alignof__", r###"extern int object __attribute__((aligned(32)));extern int object;
unsigned long long result=__alignof__(object);
"###, [Some(32); 7]),
    ("extra/evidence.json:redecl-plain:_Alignof", r###"extern int object __attribute__((aligned(32)));extern int object;
unsigned long long result=_Alignof(object);
"###, [Some(32); 7]),
    ("extra/evidence.json:undersized-alignas:__alignof__", r###"_Alignas(1) int object;
unsigned long long result=__alignof__(object);
"###, [None; 7]),
    ("extra/evidence.json:undersized-alignas:_Alignof", r###"_Alignas(1) int object;
unsigned long long result=_Alignof(object);
"###, [None; 7]),
    ("extra/evidence.json:extern-incomplete-aligned:__alignof__", r###"struct S; extern struct S object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, [Some(32), Some(32), None, None, None, None, None]),
    ("extra/evidence.json:extern-incomplete-aligned:_Alignof", r###"struct S; extern struct S object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, [Some(32), Some(32), None, None, None, None, None]),
    ("pointers/evidence.json:aligned-plus-one:__alignof__", r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object[0]+1));
"###, [Some(4); 7]),
    ("pointers/evidence.json:aligned-plus-one:_Alignof", r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object[0]+1));
"###, [Some(4); 7]),
    ("pointers/evidence.json:aligned-direct-plus-one:__alignof__", r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+1));
"###, [Some(4); 7]),
    ("pointers/evidence.json:aligned-direct-plus-one:_Alignof", r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+1));
"###, [Some(4); 7]),
    ("pointers/evidence.json:aligned-direct-plus-two:__alignof__", r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+2));
"###, [Some(4); 7]),
    ("pointers/evidence.json:aligned-direct-plus-two:_Alignof", r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+2));
"###, [Some(4); 7]),
    ("pointers/evidence.json:aligned-direct-plus-unknown:__alignof__", r###"int object __attribute__((aligned(32)));int i;
unsigned long long result=__alignof__(*(&object+i));
"###, [Some(4); 7]),
    ("pointers/evidence.json:aligned-direct-plus-unknown:_Alignof", r###"int object __attribute__((aligned(32)));int i;
unsigned long long result=_Alignof(*(&object+i));
"###, [Some(4); 7]),
    ("pointers/evidence.json:aligned-comma-pointer:__alignof__", r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(0,&object));
"###, [Some(4); 7]),
    ("pointers/evidence.json:aligned-comma-pointer:_Alignof", r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(0,&object));
"###, [Some(4); 7]),
    ("pointers/evidence.json:aligned-cast-void:__alignof__", r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*((int*)(void*)&object));
"###, [Some(32), Some(32), Some(4), Some(4), Some(4), Some(4), Some(4)]),
    ("pointers/evidence.json:aligned-cast-void:_Alignof", r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*((int*)(void*)&object));
"###, [Some(32), Some(32), Some(4), Some(4), Some(4), Some(4), Some(4)]),
    ("pointers/evidence.json:aligned-conditional-same:__alignof__", r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(1?&object:&object));
"###, [Some(4); 7]),
    ("pointers/evidence.json:aligned-conditional-same:_Alignof", r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(1?&object:&object));
"###, [Some(4); 7]),
    ("pointers/evidence.json:packed-pointer-unknown:__alignof__", r###"struct __attribute__((packed)) S{char x;int v;};struct S object;int i;
unsigned long long result=__alignof__(*(&object.v+i));
"###, [Some(4); 7]),
    ("pointers/evidence.json:packed-pointer-unknown:_Alignof", r###"struct __attribute__((packed)) S{char x;int v;};struct S object;int i;
unsigned long long result=_Alignof(*(&object.v+i));
"###, [Some(4); 7]),
    ("pointers/evidence.json:array-decay-deref:__alignof__", r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*object);
"###, [Some(4); 7]),
    ("pointers/evidence.json:array-decay-deref:_Alignof", r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*object);
"###, [Some(4); 7]),
    ("pointers/evidence.json:array-address-deref:__alignof__", r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object));
"###, [Some(32), Some(32), Some(4), Some(4), Some(4), Some(4), Some(4)]),
    ("pointers/evidence.json:array-address-deref:_Alignof", r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object));
"###, [Some(32), Some(32), Some(4), Some(4), Some(4), Some(4), Some(4)]),
];

fn observations() -> impl Iterator<Item = (&'static str, Target, Compiler, &'static str, Option<u64>)> {
    OBSERVATIONS.iter().flat_map(|&(name, source, values)| {
        QUERY_PROFILES.into_iter().zip(values).map(move |((target, compiler), value)| {
            (name, target, compiler, source, value)
        })
    })
}
