// Native GCC 13 / Clang 18 query observations; provenance is retained in the evidence report.
const OBSERVATIONS: &[(&str, Target, Compiler, &str, Option<u64>)] = &[
    ("evidence.json:object:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object;
unsigned long long result=__alignof__(object);
"###, Some(4)),
    ("evidence.json:object:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object;
unsigned long long result=__alignof__(object);
"###, Some(4)),
    ("evidence.json:object:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object;
unsigned long long result=__alignof__(object);
"###, Some(4)),
    ("evidence.json:object:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object;
unsigned long long result=__alignof__(object);
"###, Some(4)),
    ("evidence.json:object:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object;
unsigned long long result=__alignof__(object);
"###, Some(4)),
    ("evidence.json:object:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object;
unsigned long long result=__alignof__(object);
"###, Some(4)),
    ("evidence.json:object:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object;
unsigned long long result=__alignof__(object);
"###, Some(4)),
    ("evidence.json:object:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object;
unsigned long long result=_Alignof(object);
"###, Some(4)),
    ("evidence.json:object:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object;
unsigned long long result=_Alignof(object);
"###, Some(4)),
    ("evidence.json:object:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object;
unsigned long long result=_Alignof(object);
"###, Some(4)),
    ("evidence.json:object:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object;
unsigned long long result=_Alignof(object);
"###, Some(4)),
    ("evidence.json:object:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object;
unsigned long long result=_Alignof(object);
"###, Some(4)),
    ("evidence.json:object:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object;
unsigned long long result=_Alignof(object);
"###, Some(4)),
    ("evidence.json:object:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object;
unsigned long long result=_Alignof(object);
"###, Some(4)),
    ("evidence.json:aligned-object:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(16)));
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:aligned-object:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(16)));
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:aligned-object:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(16)));
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:aligned-object:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(16)));
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:aligned-object:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(16)));
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:aligned-object:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(16)));
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:aligned-object:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(16)));
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:aligned-object:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(16)));
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:aligned-object:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(16)));
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:aligned-object:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(16)));
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:aligned-object:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(16)));
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:aligned-object:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(16)));
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:aligned-object:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(16)));
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:aligned-object:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(16)));
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:lowered-object:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object);
"###, Some(1)),
    ("evidence.json:lowered-object:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object);
"###, Some(1)),
    ("evidence.json:lowered-object:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object);
"###, Some(1)),
    ("evidence.json:lowered-object:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object);
"###, Some(1)),
    ("evidence.json:lowered-object:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object);
"###, Some(1)),
    ("evidence.json:lowered-object:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object);
"###, Some(1)),
    ("evidence.json:lowered-object:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object);
"###, Some(1)),
    ("evidence.json:lowered-object:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object);
"###, Some(1)),
    ("evidence.json:lowered-object:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object);
"###, Some(1)),
    ("evidence.json:lowered-object:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object);
"###, Some(1)),
    ("evidence.json:lowered-object:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object);
"###, Some(1)),
    ("evidence.json:lowered-object:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object);
"###, Some(1)),
    ("evidence.json:lowered-object:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object);
"###, Some(1)),
    ("evidence.json:lowered-object:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object);
"###, Some(1)),
    ("evidence.json:alignas-object:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"_Alignas(32) int object;
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:alignas-object:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"_Alignas(32) int object;
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:alignas-object:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"_Alignas(32) int object;
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:alignas-object:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"_Alignas(32) int object;
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:alignas-object:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"_Alignas(32) int object;
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:alignas-object:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"_Alignas(32) int object;
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:alignas-object:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"_Alignas(32) int object;
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:alignas-object:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"_Alignas(32) int object;
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:alignas-object:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"_Alignas(32) int object;
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:alignas-object:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"_Alignas(32) int object;
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:alignas-object:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"_Alignas(32) int object;
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:alignas-object:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"_Alignas(32) int object;
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:alignas-object:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"_Alignas(32) int object;
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:alignas-object:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"_Alignas(32) int object;
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:aligned-typedef:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int T __attribute__((aligned(16)));T object;
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:aligned-typedef:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int T __attribute__((aligned(16)));T object;
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:aligned-typedef:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T object;
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:aligned-typedef:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T object;
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:aligned-typedef:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T object;
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:aligned-typedef:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T object;
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:aligned-typedef:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T object;
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:aligned-typedef:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int T __attribute__((aligned(16)));T object;
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:aligned-typedef:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int T __attribute__((aligned(16)));T object;
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:aligned-typedef:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T object;
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:aligned-typedef:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T object;
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:aligned-typedef:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T object;
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:aligned-typedef:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T object;
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:aligned-typedef:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T object;
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:packed-field:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(1)),
    ("evidence.json:packed-field:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(1)),
    ("evidence.json:packed-field:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(1)),
    ("evidence.json:packed-field:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(1)),
    ("evidence.json:packed-field:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(1)),
    ("evidence.json:packed-field:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(1)),
    ("evidence.json:packed-field:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(1)),
    ("evidence.json:packed-field:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(1)),
    ("evidence.json:packed-field:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(1)),
    ("evidence.json:packed-field:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(1)),
    ("evidence.json:packed-field:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(1)),
    ("evidence.json:packed-field:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(1)),
    ("evidence.json:packed-field:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(1)),
    ("evidence.json:packed-field:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(1)),
    ("evidence.json:aligned-field:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct S{char lead;int value __attribute__((aligned(16)));};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(16)),
    ("evidence.json:aligned-field:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct S{char lead;int value __attribute__((aligned(16)));};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(16)),
    ("evidence.json:aligned-field:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct S{char lead;int value __attribute__((aligned(16)));};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(16)),
    ("evidence.json:aligned-field:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct S{char lead;int value __attribute__((aligned(16)));};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(16)),
    ("evidence.json:aligned-field:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct S{char lead;int value __attribute__((aligned(16)));};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(16)),
    ("evidence.json:aligned-field:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct S{char lead;int value __attribute__((aligned(16)));};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(16)),
    ("evidence.json:aligned-field:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct S{char lead;int value __attribute__((aligned(16)));};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(16)),
    ("evidence.json:aligned-field:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct S{char lead;int value __attribute__((aligned(16)));};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(16)),
    ("evidence.json:aligned-field:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct S{char lead;int value __attribute__((aligned(16)));};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(16)),
    ("evidence.json:aligned-field:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct S{char lead;int value __attribute__((aligned(16)));};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(16)),
    ("evidence.json:aligned-field:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct S{char lead;int value __attribute__((aligned(16)));};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(16)),
    ("evidence.json:aligned-field:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct S{char lead;int value __attribute__((aligned(16)));};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(16)),
    ("evidence.json:aligned-field:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct S{char lead;int value __attribute__((aligned(16)));};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(16)),
    ("evidence.json:aligned-field:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct S{char lead;int value __attribute__((aligned(16)));};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(16)),
    ("evidence.json:packed-aligned-field:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char lead;int value __attribute__((aligned(8)));};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(8)),
    ("evidence.json:packed-aligned-field:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char lead;int value __attribute__((aligned(8)));};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(8)),
    ("evidence.json:packed-aligned-field:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value __attribute__((aligned(8)));};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(8)),
    ("evidence.json:packed-aligned-field:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value __attribute__((aligned(8)));};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(8)),
    ("evidence.json:packed-aligned-field:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value __attribute__((aligned(8)));};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(8)),
    ("evidence.json:packed-aligned-field:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value __attribute__((aligned(8)));};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(8)),
    ("evidence.json:packed-aligned-field:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value __attribute__((aligned(8)));};struct S object;
unsigned long long result=__alignof__(object.value);
"###, Some(8)),
    ("evidence.json:packed-aligned-field:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char lead;int value __attribute__((aligned(8)));};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(8)),
    ("evidence.json:packed-aligned-field:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char lead;int value __attribute__((aligned(8)));};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(8)),
    ("evidence.json:packed-aligned-field:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value __attribute__((aligned(8)));};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(8)),
    ("evidence.json:packed-aligned-field:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value __attribute__((aligned(8)));};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(8)),
    ("evidence.json:packed-aligned-field:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value __attribute__((aligned(8)));};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(8)),
    ("evidence.json:packed-aligned-field:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value __attribute__((aligned(8)));};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(8)),
    ("evidence.json:packed-aligned-field:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value __attribute__((aligned(8)));};struct S object;
unsigned long long result=_Alignof(object.value);
"###, Some(8)),
    ("evidence.json:nested-packed:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct Inner{char lead;int value;};struct __attribute__((packed)) Outer{char lead;struct Inner inner;};struct Outer object;
unsigned long long result=__alignof__(object.inner.value);
"###, Some(4)),
    ("evidence.json:nested-packed:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct Inner{char lead;int value;};struct __attribute__((packed)) Outer{char lead;struct Inner inner;};struct Outer object;
unsigned long long result=__alignof__(object.inner.value);
"###, Some(4)),
    ("evidence.json:nested-packed:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct Inner{char lead;int value;};struct __attribute__((packed)) Outer{char lead;struct Inner inner;};struct Outer object;
unsigned long long result=__alignof__(object.inner.value);
"###, Some(4)),
    ("evidence.json:nested-packed:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct Inner{char lead;int value;};struct __attribute__((packed)) Outer{char lead;struct Inner inner;};struct Outer object;
unsigned long long result=__alignof__(object.inner.value);
"###, Some(4)),
    ("evidence.json:nested-packed:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct Inner{char lead;int value;};struct __attribute__((packed)) Outer{char lead;struct Inner inner;};struct Outer object;
unsigned long long result=__alignof__(object.inner.value);
"###, Some(4)),
    ("evidence.json:nested-packed:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct Inner{char lead;int value;};struct __attribute__((packed)) Outer{char lead;struct Inner inner;};struct Outer object;
unsigned long long result=__alignof__(object.inner.value);
"###, Some(4)),
    ("evidence.json:nested-packed:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct Inner{char lead;int value;};struct __attribute__((packed)) Outer{char lead;struct Inner inner;};struct Outer object;
unsigned long long result=__alignof__(object.inner.value);
"###, Some(4)),
    ("evidence.json:nested-packed:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct Inner{char lead;int value;};struct __attribute__((packed)) Outer{char lead;struct Inner inner;};struct Outer object;
unsigned long long result=_Alignof(object.inner.value);
"###, Some(4)),
    ("evidence.json:nested-packed:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct Inner{char lead;int value;};struct __attribute__((packed)) Outer{char lead;struct Inner inner;};struct Outer object;
unsigned long long result=_Alignof(object.inner.value);
"###, Some(4)),
    ("evidence.json:nested-packed:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct Inner{char lead;int value;};struct __attribute__((packed)) Outer{char lead;struct Inner inner;};struct Outer object;
unsigned long long result=_Alignof(object.inner.value);
"###, Some(4)),
    ("evidence.json:nested-packed:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct Inner{char lead;int value;};struct __attribute__((packed)) Outer{char lead;struct Inner inner;};struct Outer object;
unsigned long long result=_Alignof(object.inner.value);
"###, Some(4)),
    ("evidence.json:nested-packed:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct Inner{char lead;int value;};struct __attribute__((packed)) Outer{char lead;struct Inner inner;};struct Outer object;
unsigned long long result=_Alignof(object.inner.value);
"###, Some(4)),
    ("evidence.json:nested-packed:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct Inner{char lead;int value;};struct __attribute__((packed)) Outer{char lead;struct Inner inner;};struct Outer object;
unsigned long long result=_Alignof(object.inner.value);
"###, Some(4)),
    ("evidence.json:nested-packed:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct Inner{char lead;int value;};struct __attribute__((packed)) Outer{char lead;struct Inner inner;};struct Outer object;
unsigned long long result=_Alignof(object.inner.value);
"###, Some(4)),
    ("evidence.json:aligned-container:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct S{int value;};struct S object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object.value);
"###, Some(4)),
    ("evidence.json:aligned-container:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct S{int value;};struct S object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object.value);
"###, Some(4)),
    ("evidence.json:aligned-container:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct S{int value;};struct S object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object.value);
"###, Some(4)),
    ("evidence.json:aligned-container:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct S{int value;};struct S object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object.value);
"###, Some(4)),
    ("evidence.json:aligned-container:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct S{int value;};struct S object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object.value);
"###, Some(4)),
    ("evidence.json:aligned-container:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct S{int value;};struct S object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object.value);
"###, Some(4)),
    ("evidence.json:aligned-container:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct S{int value;};struct S object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object.value);
"###, Some(4)),
    ("evidence.json:aligned-container:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct S{int value;};struct S object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object.value);
"###, Some(4)),
    ("evidence.json:aligned-container:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct S{int value;};struct S object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object.value);
"###, Some(4)),
    ("evidence.json:aligned-container:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct S{int value;};struct S object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object.value);
"###, Some(4)),
    ("evidence.json:aligned-container:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct S{int value;};struct S object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object.value);
"###, Some(4)),
    ("evidence.json:aligned-container:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct S{int value;};struct S object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object.value);
"###, Some(4)),
    ("evidence.json:aligned-container:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct S{int value;};struct S object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object.value);
"###, Some(4)),
    ("evidence.json:aligned-container:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct S{int value;};struct S object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object.value);
"###, Some(4)),
    ("evidence.json:packed-address:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=__alignof__(*(&object.value));
"###, Some(1)),
    ("evidence.json:packed-address:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=__alignof__(*(&object.value));
"###, Some(1)),
    ("evidence.json:packed-address:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=__alignof__(*(&object.value));
"###, Some(4)),
    ("evidence.json:packed-address:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=__alignof__(*(&object.value));
"###, Some(4)),
    ("evidence.json:packed-address:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=__alignof__(*(&object.value));
"###, Some(4)),
    ("evidence.json:packed-address:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=__alignof__(*(&object.value));
"###, Some(4)),
    ("evidence.json:packed-address:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=__alignof__(*(&object.value));
"###, Some(4)),
    ("evidence.json:packed-address:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=_Alignof(*(&object.value));
"###, Some(1)),
    ("evidence.json:packed-address:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=_Alignof(*(&object.value));
"###, Some(1)),
    ("evidence.json:packed-address:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=_Alignof(*(&object.value));
"###, Some(4)),
    ("evidence.json:packed-address:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=_Alignof(*(&object.value));
"###, Some(4)),
    ("evidence.json:packed-address:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=_Alignof(*(&object.value));
"###, Some(4)),
    ("evidence.json:packed-address:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=_Alignof(*(&object.value));
"###, Some(4)),
    ("evidence.json:packed-address:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct __attribute__((packed)) S{char lead;int value;};struct S object;
unsigned long long result=_Alignof(*(&object.value));
"###, Some(4)),
    ("evidence.json:array:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:array:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:array:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:array:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:array:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:array:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:array:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:array:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:array:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:array:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:array:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:array:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:array:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:array:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:element-zero:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:element-zero:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:element-zero:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:element-zero:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:element-zero:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:element-zero:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:element-zero:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:element-zero:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:element-zero:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:element-zero:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:element-zero:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:element-zero:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:element-zero:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:element-zero:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:element-one:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object[1]);
"###, Some(4)),
    ("evidence.json:element-one:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object[1]);
"###, Some(4)),
    ("evidence.json:element-one:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object[1]);
"###, Some(4)),
    ("evidence.json:element-one:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object[1]);
"###, Some(4)),
    ("evidence.json:element-one:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object[1]);
"###, Some(4)),
    ("evidence.json:element-one:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object[1]);
"###, Some(4)),
    ("evidence.json:element-one:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(object[1]);
"###, Some(4)),
    ("evidence.json:element-one:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object[1]);
"###, Some(4)),
    ("evidence.json:element-one:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object[1]);
"###, Some(4)),
    ("evidence.json:element-one:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object[1]);
"###, Some(4)),
    ("evidence.json:element-one:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object[1]);
"###, Some(4)),
    ("evidence.json:element-one:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object[1]);
"###, Some(4)),
    ("evidence.json:element-one:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object[1]);
"###, Some(4)),
    ("evidence.json:element-one:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(object[1]);
"###, Some(4)),
    ("evidence.json:element-variable:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));int i;
unsigned long long result=__alignof__(object[i]);
"###, Some(4)),
    ("evidence.json:element-variable:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));int i;
unsigned long long result=__alignof__(object[i]);
"###, Some(4)),
    ("evidence.json:element-variable:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));int i;
unsigned long long result=__alignof__(object[i]);
"###, Some(4)),
    ("evidence.json:element-variable:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));int i;
unsigned long long result=__alignof__(object[i]);
"###, Some(4)),
    ("evidence.json:element-variable:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));int i;
unsigned long long result=__alignof__(object[i]);
"###, Some(4)),
    ("evidence.json:element-variable:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));int i;
unsigned long long result=__alignof__(object[i]);
"###, Some(4)),
    ("evidence.json:element-variable:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));int i;
unsigned long long result=__alignof__(object[i]);
"###, Some(4)),
    ("evidence.json:element-variable:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));int i;
unsigned long long result=_Alignof(object[i]);
"###, Some(4)),
    ("evidence.json:element-variable:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));int i;
unsigned long long result=_Alignof(object[i]);
"###, Some(4)),
    ("evidence.json:element-variable:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));int i;
unsigned long long result=_Alignof(object[i]);
"###, Some(4)),
    ("evidence.json:element-variable:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));int i;
unsigned long long result=_Alignof(object[i]);
"###, Some(4)),
    ("evidence.json:element-variable:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));int i;
unsigned long long result=_Alignof(object[i]);
"###, Some(4)),
    ("evidence.json:element-variable:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));int i;
unsigned long long result=_Alignof(object[i]);
"###, Some(4)),
    ("evidence.json:element-variable:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));int i;
unsigned long long result=_Alignof(object[i]);
"###, Some(4)),
    ("evidence.json:dereference:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int *object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("evidence.json:dereference:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int *object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("evidence.json:dereference:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int *object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("evidence.json:dereference:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int *object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("evidence.json:dereference:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int *object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("evidence.json:dereference:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int *object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("evidence.json:dereference:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int *object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("evidence.json:dereference:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int *object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("evidence.json:dereference:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int *object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("evidence.json:dereference:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int *object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("evidence.json:dereference:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int *object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("evidence.json:dereference:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int *object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("evidence.json:dereference:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int *object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("evidence.json:dereference:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int *object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("evidence.json:aligned-pointee:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int T __attribute__((aligned(16)));T *object;
unsigned long long result=__alignof__(*object);
"###, Some(16)),
    ("evidence.json:aligned-pointee:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int T __attribute__((aligned(16)));T *object;
unsigned long long result=__alignof__(*object);
"###, Some(16)),
    ("evidence.json:aligned-pointee:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T *object;
unsigned long long result=__alignof__(*object);
"###, Some(16)),
    ("evidence.json:aligned-pointee:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T *object;
unsigned long long result=__alignof__(*object);
"###, Some(16)),
    ("evidence.json:aligned-pointee:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T *object;
unsigned long long result=__alignof__(*object);
"###, Some(16)),
    ("evidence.json:aligned-pointee:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T *object;
unsigned long long result=__alignof__(*object);
"###, Some(16)),
    ("evidence.json:aligned-pointee:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T *object;
unsigned long long result=__alignof__(*object);
"###, Some(16)),
    ("evidence.json:aligned-pointee:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int T __attribute__((aligned(16)));T *object;
unsigned long long result=_Alignof(*object);
"###, Some(16)),
    ("evidence.json:aligned-pointee:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int T __attribute__((aligned(16)));T *object;
unsigned long long result=_Alignof(*object);
"###, Some(16)),
    ("evidence.json:aligned-pointee:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T *object;
unsigned long long result=_Alignof(*object);
"###, Some(16)),
    ("evidence.json:aligned-pointee:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T *object;
unsigned long long result=_Alignof(*object);
"###, Some(16)),
    ("evidence.json:aligned-pointee:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T *object;
unsigned long long result=_Alignof(*object);
"###, Some(16)),
    ("evidence.json:aligned-pointee:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T *object;
unsigned long long result=_Alignof(*object);
"###, Some(16)),
    ("evidence.json:aligned-pointee:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));T *object;
unsigned long long result=_Alignof(*object);
"###, Some(16)),
    ("evidence.json:cast:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=__alignof__((T)0);
"###, Some(4)),
    ("evidence.json:cast:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=__alignof__((T)0);
"###, Some(4)),
    ("evidence.json:cast:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=__alignof__((T)0);
"###, Some(16)),
    ("evidence.json:cast:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=__alignof__((T)0);
"###, Some(16)),
    ("evidence.json:cast:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=__alignof__((T)0);
"###, Some(16)),
    ("evidence.json:cast:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=__alignof__((T)0);
"###, Some(16)),
    ("evidence.json:cast:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=__alignof__((T)0);
"###, Some(16)),
    ("evidence.json:cast:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=_Alignof((T)0);
"###, Some(4)),
    ("evidence.json:cast:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=_Alignof((T)0);
"###, Some(4)),
    ("evidence.json:cast:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=_Alignof((T)0);
"###, Some(16)),
    ("evidence.json:cast:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=_Alignof((T)0);
"###, Some(16)),
    ("evidence.json:cast:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=_Alignof((T)0);
"###, Some(16)),
    ("evidence.json:cast:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=_Alignof((T)0);
"###, Some(16)),
    ("evidence.json:cast:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=_Alignof((T)0);
"###, Some(16)),
    ("evidence.json:compound-literal:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=__alignof__((T){0});
"###, Some(16)),
    ("evidence.json:compound-literal:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=__alignof__((T){0});
"###, Some(16)),
    ("evidence.json:compound-literal:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=__alignof__((T){0});
"###, Some(16)),
    ("evidence.json:compound-literal:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=__alignof__((T){0});
"###, Some(16)),
    ("evidence.json:compound-literal:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=__alignof__((T){0});
"###, Some(16)),
    ("evidence.json:compound-literal:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=__alignof__((T){0});
"###, Some(16)),
    ("evidence.json:compound-literal:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=__alignof__((T){0});
"###, Some(16)),
    ("evidence.json:compound-literal:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=_Alignof((T){0});
"###, Some(16)),
    ("evidence.json:compound-literal:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=_Alignof((T){0});
"###, Some(16)),
    ("evidence.json:compound-literal:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=_Alignof((T){0});
"###, Some(16)),
    ("evidence.json:compound-literal:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=_Alignof((T){0});
"###, Some(16)),
    ("evidence.json:compound-literal:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=_Alignof((T){0});
"###, Some(16)),
    ("evidence.json:compound-literal:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=_Alignof((T){0});
"###, Some(16)),
    ("evidence.json:compound-literal:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"typedef int T __attribute__((aligned(16)));
unsigned long long result=_Alignof((T){0});
"###, Some(16)),
    ("evidence.json:conditional:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=__alignof__(1?object:plain);
"###, Some(4)),
    ("evidence.json:conditional:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=__alignof__(1?object:plain);
"###, Some(4)),
    ("evidence.json:conditional:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=__alignof__(1?object:plain);
"###, Some(4)),
    ("evidence.json:conditional:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=__alignof__(1?object:plain);
"###, Some(4)),
    ("evidence.json:conditional:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=__alignof__(1?object:plain);
"###, Some(4)),
    ("evidence.json:conditional:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=__alignof__(1?object:plain);
"###, Some(4)),
    ("evidence.json:conditional:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=__alignof__(1?object:plain);
"###, Some(4)),
    ("evidence.json:conditional:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=_Alignof(1?object:plain);
"###, Some(4)),
    ("evidence.json:conditional:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=_Alignof(1?object:plain);
"###, Some(4)),
    ("evidence.json:conditional:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=_Alignof(1?object:plain);
"###, Some(4)),
    ("evidence.json:conditional:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=_Alignof(1?object:plain);
"###, Some(4)),
    ("evidence.json:conditional:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=_Alignof(1?object:plain);
"###, Some(4)),
    ("evidence.json:conditional:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=_Alignof(1?object:plain);
"###, Some(4)),
    ("evidence.json:conditional:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=_Alignof(1?object:plain);
"###, Some(4)),
    ("evidence.json:comma:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(0,object);
"###, Some(4)),
    ("evidence.json:comma:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(0,object);
"###, Some(4)),
    ("evidence.json:comma:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(0,object);
"###, Some(4)),
    ("evidence.json:comma:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(0,object);
"###, Some(4)),
    ("evidence.json:comma:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(0,object);
"###, Some(4)),
    ("evidence.json:comma:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(0,object);
"###, Some(4)),
    ("evidence.json:comma:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(0,object);
"###, Some(4)),
    ("evidence.json:comma:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(0,object);
"###, Some(4)),
    ("evidence.json:comma:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(0,object);
"###, Some(4)),
    ("evidence.json:comma:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(0,object);
"###, Some(4)),
    ("evidence.json:comma:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(0,object);
"###, Some(4)),
    ("evidence.json:comma:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(0,object);
"###, Some(4)),
    ("evidence.json:comma:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(0,object);
"###, Some(4)),
    ("evidence.json:comma:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(0,object);
"###, Some(4)),
    ("evidence.json:generic:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(_Generic(0,int:object,default:1));
"###, Some(32)),
    ("evidence.json:generic:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(_Generic(0,int:object,default:1));
"###, Some(32)),
    ("evidence.json:generic:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(_Generic(0,int:object,default:1));
"###, Some(32)),
    ("evidence.json:generic:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(_Generic(0,int:object,default:1));
"###, Some(32)),
    ("evidence.json:generic:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(_Generic(0,int:object,default:1));
"###, Some(32)),
    ("evidence.json:generic:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(_Generic(0,int:object,default:1));
"###, Some(32)),
    ("evidence.json:generic:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(_Generic(0,int:object,default:1));
"###, Some(32)),
    ("evidence.json:generic:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(_Generic(0,int:object,default:1));
"###, Some(32)),
    ("evidence.json:generic:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(_Generic(0,int:object,default:1));
"###, Some(32)),
    ("evidence.json:generic:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(_Generic(0,int:object,default:1));
"###, Some(32)),
    ("evidence.json:generic:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(_Generic(0,int:object,default:1));
"###, Some(32)),
    ("evidence.json:generic:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(_Generic(0,int:object,default:1));
"###, Some(32)),
    ("evidence.json:generic:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(_Generic(0,int:object,default:1));
"###, Some(32)),
    ("evidence.json:generic:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(_Generic(0,int:object,default:1));
"###, Some(32)),
    ("evidence.json:literal:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"
unsigned long long result=__alignof__(3);
"###, Some(4)),
    ("evidence.json:literal:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"
unsigned long long result=__alignof__(3);
"###, Some(4)),
    ("evidence.json:literal:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"
unsigned long long result=__alignof__(3);
"###, Some(4)),
    ("evidence.json:literal:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"
unsigned long long result=__alignof__(3);
"###, Some(4)),
    ("evidence.json:literal:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"
unsigned long long result=__alignof__(3);
"###, Some(4)),
    ("evidence.json:literal:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"
unsigned long long result=__alignof__(3);
"###, Some(4)),
    ("evidence.json:literal:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"
unsigned long long result=__alignof__(3);
"###, Some(4)),
    ("evidence.json:literal:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"
unsigned long long result=_Alignof(3);
"###, Some(4)),
    ("evidence.json:literal:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"
unsigned long long result=_Alignof(3);
"###, Some(4)),
    ("evidence.json:literal:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"
unsigned long long result=_Alignof(3);
"###, Some(4)),
    ("evidence.json:literal:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"
unsigned long long result=_Alignof(3);
"###, Some(4)),
    ("evidence.json:literal:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"
unsigned long long result=_Alignof(3);
"###, Some(4)),
    ("evidence.json:literal:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"
unsigned long long result=_Alignof(3);
"###, Some(4)),
    ("evidence.json:literal:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"
unsigned long long result=_Alignof(3);
"###, Some(4)),
    ("evidence.json:string:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"
unsigned long long result=__alignof__("abc");
"###, Some(1)),
    ("evidence.json:string:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"
unsigned long long result=__alignof__("abc");
"###, Some(1)),
    ("evidence.json:string:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"
unsigned long long result=__alignof__("abc");
"###, Some(1)),
    ("evidence.json:string:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"
unsigned long long result=__alignof__("abc");
"###, Some(1)),
    ("evidence.json:string:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"
unsigned long long result=__alignof__("abc");
"###, Some(1)),
    ("evidence.json:string:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"
unsigned long long result=__alignof__("abc");
"###, Some(1)),
    ("evidence.json:string:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"
unsigned long long result=__alignof__("abc");
"###, Some(1)),
    ("evidence.json:string:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"
unsigned long long result=_Alignof("abc");
"###, Some(1)),
    ("evidence.json:string:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"
unsigned long long result=_Alignof("abc");
"###, Some(1)),
    ("evidence.json:string:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"
unsigned long long result=_Alignof("abc");
"###, Some(1)),
    ("evidence.json:string:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"
unsigned long long result=_Alignof("abc");
"###, Some(1)),
    ("evidence.json:string:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"
unsigned long long result=_Alignof("abc");
"###, Some(1)),
    ("evidence.json:string:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"
unsigned long long result=_Alignof("abc");
"###, Some(1)),
    ("evidence.json:string:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"
unsigned long long result=_Alignof("abc");
"###, Some(1)),
    ("evidence.json:function:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"void object(void);
unsigned long long result=__alignof__(object);
"###, Some(1)),
    ("evidence.json:function:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"void object(void);
unsigned long long result=__alignof__(object);
"###, Some(4)),
    ("evidence.json:function:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"void object(void);
unsigned long long result=__alignof__(object);
"###, Some(4)),
    ("evidence.json:function:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"void object(void);
unsigned long long result=__alignof__(object);
"###, Some(4)),
    ("evidence.json:function:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"void object(void);
unsigned long long result=__alignof__(object);
"###, Some(4)),
    ("evidence.json:function:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"void object(void);
unsigned long long result=__alignof__(object);
"###, Some(4)),
    ("evidence.json:function:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"void object(void);
unsigned long long result=__alignof__(object);
"###, Some(4)),
    ("evidence.json:function:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"void object(void);
unsigned long long result=_Alignof(object);
"###, Some(1)),
    ("evidence.json:function:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"void object(void);
unsigned long long result=_Alignof(object);
"###, Some(4)),
    ("evidence.json:function:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"void object(void);
unsigned long long result=_Alignof(object);
"###, Some(4)),
    ("evidence.json:function:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"void object(void);
unsigned long long result=_Alignof(object);
"###, Some(4)),
    ("evidence.json:function:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"void object(void);
unsigned long long result=_Alignof(object);
"###, Some(4)),
    ("evidence.json:function:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"void object(void);
unsigned long long result=_Alignof(object);
"###, Some(4)),
    ("evidence.json:function:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"void object(void);
unsigned long long result=_Alignof(object);
"###, Some(4)),
    ("evidence.json:aligned-function:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"void object(void) __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:aligned-function:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"void object(void) __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:aligned-function:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"void object(void) __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:aligned-function:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"void object(void) __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:aligned-function:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"void object(void) __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:aligned-function:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"void object(void) __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:aligned-function:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"void object(void) __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("evidence.json:aligned-function:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"void object(void) __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:aligned-function:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"void object(void) __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:aligned-function:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"void object(void) __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:aligned-function:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"void object(void) __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:aligned-function:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"void object(void) __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:aligned-function:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"void object(void) __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:aligned-function:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"void object(void) __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("evidence.json:function-pointer:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"void (*object)(void);
unsigned long long result=__alignof__(*object);
"###, Some(1)),
    ("evidence.json:function-pointer:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"void (*object)(void);
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("evidence.json:function-pointer:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"void (*object)(void);
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("evidence.json:function-pointer:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"void (*object)(void);
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("evidence.json:function-pointer:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"void (*object)(void);
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("evidence.json:function-pointer:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"void (*object)(void);
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("evidence.json:function-pointer:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"void (*object)(void);
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("evidence.json:function-pointer:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"void (*object)(void);
unsigned long long result=_Alignof(*object);
"###, Some(1)),
    ("evidence.json:function-pointer:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"void (*object)(void);
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("evidence.json:function-pointer:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"void (*object)(void);
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("evidence.json:function-pointer:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"void (*object)(void);
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("evidence.json:function-pointer:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"void (*object)(void);
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("evidence.json:function-pointer:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"void (*object)(void);
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("evidence.json:function-pointer:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"void (*object)(void);
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("evidence.json:function-call:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object(void);
unsigned long long result=__alignof__(object());
"###, Some(4)),
    ("evidence.json:function-call:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object(void);
unsigned long long result=__alignof__(object());
"###, Some(4)),
    ("evidence.json:function-call:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object(void);
unsigned long long result=__alignof__(object());
"###, Some(4)),
    ("evidence.json:function-call:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object(void);
unsigned long long result=__alignof__(object());
"###, Some(4)),
    ("evidence.json:function-call:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object(void);
unsigned long long result=__alignof__(object());
"###, Some(4)),
    ("evidence.json:function-call:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object(void);
unsigned long long result=__alignof__(object());
"###, Some(4)),
    ("evidence.json:function-call:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object(void);
unsigned long long result=__alignof__(object());
"###, Some(4)),
    ("evidence.json:function-call:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object(void);
unsigned long long result=_Alignof(object());
"###, Some(4)),
    ("evidence.json:function-call:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object(void);
unsigned long long result=_Alignof(object());
"###, Some(4)),
    ("evidence.json:function-call:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object(void);
unsigned long long result=_Alignof(object());
"###, Some(4)),
    ("evidence.json:function-call:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object(void);
unsigned long long result=_Alignof(object());
"###, Some(4)),
    ("evidence.json:function-call:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object(void);
unsigned long long result=_Alignof(object());
"###, Some(4)),
    ("evidence.json:function-call:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object(void);
unsigned long long result=_Alignof(object());
"###, Some(4)),
    ("evidence.json:function-call:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object(void);
unsigned long long result=_Alignof(object());
"###, Some(4)),
    ("evidence.json:void:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"
unsigned long long result=__alignof__((void)0);
"###, Some(1)),
    ("evidence.json:void:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"
unsigned long long result=__alignof__((void)0);
"###, Some(1)),
    ("evidence.json:void:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"
unsigned long long result=__alignof__((void)0);
"###, Some(1)),
    ("evidence.json:void:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"
unsigned long long result=__alignof__((void)0);
"###, Some(1)),
    ("evidence.json:void:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"
unsigned long long result=__alignof__((void)0);
"###, Some(1)),
    ("evidence.json:void:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"
unsigned long long result=__alignof__((void)0);
"###, Some(1)),
    ("evidence.json:void:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"
unsigned long long result=__alignof__((void)0);
"###, Some(1)),
    ("evidence.json:void:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"
unsigned long long result=_Alignof((void)0);
"###, Some(1)),
    ("evidence.json:void:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"
unsigned long long result=_Alignof((void)0);
"###, Some(1)),
    ("evidence.json:void:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"
unsigned long long result=_Alignof((void)0);
"###, Some(1)),
    ("evidence.json:void:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"
unsigned long long result=_Alignof((void)0);
"###, Some(1)),
    ("evidence.json:void:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"
unsigned long long result=_Alignof((void)0);
"###, Some(1)),
    ("evidence.json:void:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"
unsigned long long result=_Alignof((void)0);
"###, Some(1)),
    ("evidence.json:void:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"
unsigned long long result=_Alignof((void)0);
"###, Some(1)),
    ("evidence.json:incomplete:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct S;extern struct S object;
unsigned long long result=__alignof__(object);
"###, Some(1)),
    ("evidence.json:incomplete:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct S;extern struct S object;
unsigned long long result=__alignof__(object);
"###, Some(1)),
    ("evidence.json:incomplete:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct S;extern struct S object;
unsigned long long result=__alignof__(object);
"###, None),
    ("evidence.json:incomplete:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct S;extern struct S object;
unsigned long long result=__alignof__(object);
"###, None),
    ("evidence.json:incomplete:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct S;extern struct S object;
unsigned long long result=__alignof__(object);
"###, None),
    ("evidence.json:incomplete:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct S;extern struct S object;
unsigned long long result=__alignof__(object);
"###, None),
    ("evidence.json:incomplete:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct S;extern struct S object;
unsigned long long result=__alignof__(object);
"###, None),
    ("evidence.json:incomplete:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct S;extern struct S object;
unsigned long long result=_Alignof(object);
"###, Some(1)),
    ("evidence.json:incomplete:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct S;extern struct S object;
unsigned long long result=_Alignof(object);
"###, Some(1)),
    ("evidence.json:incomplete:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct S;extern struct S object;
unsigned long long result=_Alignof(object);
"###, None),
    ("evidence.json:incomplete:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct S;extern struct S object;
unsigned long long result=_Alignof(object);
"###, None),
    ("evidence.json:incomplete:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct S;extern struct S object;
unsigned long long result=_Alignof(object);
"###, None),
    ("evidence.json:incomplete:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct S;extern struct S object;
unsigned long long result=_Alignof(object);
"###, None),
    ("evidence.json:incomplete:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct S;extern struct S object;
unsigned long long result=_Alignof(object);
"###, None),
    ("evidence.json:bitfield:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct S{unsigned value:3;};struct S object;
unsigned long long result=__alignof__(object.value);
"###, None),
    ("evidence.json:bitfield:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct S{unsigned value:3;};struct S object;
unsigned long long result=__alignof__(object.value);
"###, None),
    ("evidence.json:bitfield:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct S{unsigned value:3;};struct S object;
unsigned long long result=__alignof__(object.value);
"###, None),
    ("evidence.json:bitfield:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct S{unsigned value:3;};struct S object;
unsigned long long result=__alignof__(object.value);
"###, None),
    ("evidence.json:bitfield:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct S{unsigned value:3;};struct S object;
unsigned long long result=__alignof__(object.value);
"###, None),
    ("evidence.json:bitfield:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct S{unsigned value:3;};struct S object;
unsigned long long result=__alignof__(object.value);
"###, None),
    ("evidence.json:bitfield:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct S{unsigned value:3;};struct S object;
unsigned long long result=__alignof__(object.value);
"###, None),
    ("evidence.json:bitfield:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct S{unsigned value:3;};struct S object;
unsigned long long result=_Alignof(object.value);
"###, None),
    ("evidence.json:bitfield:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct S{unsigned value:3;};struct S object;
unsigned long long result=_Alignof(object.value);
"###, None),
    ("evidence.json:bitfield:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct S{unsigned value:3;};struct S object;
unsigned long long result=_Alignof(object.value);
"###, None),
    ("evidence.json:bitfield:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct S{unsigned value:3;};struct S object;
unsigned long long result=_Alignof(object.value);
"###, None),
    ("evidence.json:bitfield:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct S{unsigned value:3;};struct S object;
unsigned long long result=_Alignof(object.value);
"###, None),
    ("evidence.json:bitfield:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct S{unsigned value:3;};struct S object;
unsigned long long result=_Alignof(object.value);
"###, None),
    ("evidence.json:bitfield:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct S{unsigned value:3;};struct S object;
unsigned long long result=_Alignof(object.value);
"###, None),
    ("evidence.json:vector:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:vector:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:vector:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:vector:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:vector:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:vector:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:vector:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=__alignof__(object);
"###, Some(16)),
    ("evidence.json:vector:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:vector:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:vector:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:vector:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:vector:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:vector:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:vector:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=_Alignof(object);
"###, Some(16)),
    ("evidence.json:vector-lane:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:vector-lane:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:vector-lane:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:vector-lane:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:vector-lane:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:vector-lane:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:vector-lane:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:vector-lane:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:vector-lane:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:vector-lane:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:vector-lane:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:vector-lane:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:vector-lane:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:vector-lane:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object;
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:lowered-vector:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object);
"###, Some(1)),
    ("evidence.json:lowered-vector:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object);
"###, Some(1)),
    ("evidence.json:lowered-vector:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object);
"###, Some(1)),
    ("evidence.json:lowered-vector:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object);
"###, Some(1)),
    ("evidence.json:lowered-vector:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object);
"###, Some(1)),
    ("evidence.json:lowered-vector:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object);
"###, Some(1)),
    ("evidence.json:lowered-vector:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object);
"###, Some(1)),
    ("evidence.json:lowered-vector:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object);
"###, Some(1)),
    ("evidence.json:lowered-vector:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object);
"###, Some(1)),
    ("evidence.json:lowered-vector:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object);
"###, Some(1)),
    ("evidence.json:lowered-vector:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object);
"###, Some(1)),
    ("evidence.json:lowered-vector:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object);
"###, Some(1)),
    ("evidence.json:lowered-vector:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object);
"###, Some(1)),
    ("evidence.json:lowered-vector:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object);
"###, Some(1)),
    ("evidence.json:lowered-vector-lane:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:lowered-vector-lane:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:lowered-vector-lane:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:lowered-vector-lane:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:lowered-vector-lane:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:lowered-vector-lane:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:lowered-vector-lane:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=__alignof__(object[0]);
"###, Some(4)),
    ("evidence.json:lowered-vector-lane:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:lowered-vector-lane:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:lowered-vector-lane:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:lowered-vector-lane:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:lowered-vector-lane:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:lowered-vector-lane:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("evidence.json:lowered-vector-lane:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"typedef int V __attribute__((vector_size(16)));V object __attribute__((aligned(1)));
unsigned long long result=_Alignof(object[0]);
"###, Some(4)),
    ("extra/evidence.json:packed-plus:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=__alignof__(*(&object.v+0));
"###, Some(1)),
    ("extra/evidence.json:packed-plus:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=__alignof__(*(&object.v+0));
"###, Some(1)),
    ("extra/evidence.json:packed-plus:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=__alignof__(*(&object.v+0));
"###, Some(4)),
    ("extra/evidence.json:packed-plus:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=__alignof__(*(&object.v+0));
"###, Some(4)),
    ("extra/evidence.json:packed-plus:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=__alignof__(*(&object.v+0));
"###, Some(4)),
    ("extra/evidence.json:packed-plus:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=__alignof__(*(&object.v+0));
"###, Some(4)),
    ("extra/evidence.json:packed-plus:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=__alignof__(*(&object.v+0));
"###, Some(4)),
    ("extra/evidence.json:packed-plus:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=_Alignof(*(&object.v+0));
"###, Some(1)),
    ("extra/evidence.json:packed-plus:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=_Alignof(*(&object.v+0));
"###, Some(1)),
    ("extra/evidence.json:packed-plus:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=_Alignof(*(&object.v+0));
"###, Some(4)),
    ("extra/evidence.json:packed-plus:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=_Alignof(*(&object.v+0));
"###, Some(4)),
    ("extra/evidence.json:packed-plus:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=_Alignof(*(&object.v+0));
"###, Some(4)),
    ("extra/evidence.json:packed-plus:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=_Alignof(*(&object.v+0));
"###, Some(4)),
    ("extra/evidence.json:packed-plus:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=_Alignof(*(&object.v+0));
"###, Some(4)),
    ("extra/evidence.json:aligned-roundtrip:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object));
"###, Some(32)),
    ("extra/evidence.json:aligned-roundtrip:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object));
"###, Some(32)),
    ("extra/evidence.json:aligned-roundtrip:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object));
"###, Some(4)),
    ("extra/evidence.json:aligned-roundtrip:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object));
"###, Some(4)),
    ("extra/evidence.json:aligned-roundtrip:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object));
"###, Some(4)),
    ("extra/evidence.json:aligned-roundtrip:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object));
"###, Some(4)),
    ("extra/evidence.json:aligned-roundtrip:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object));
"###, Some(4)),
    ("extra/evidence.json:aligned-roundtrip:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object));
"###, Some(32)),
    ("extra/evidence.json:aligned-roundtrip:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object));
"###, Some(32)),
    ("extra/evidence.json:aligned-roundtrip:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object));
"###, Some(4)),
    ("extra/evidence.json:aligned-roundtrip:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object));
"###, Some(4)),
    ("extra/evidence.json:aligned-roundtrip:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object));
"###, Some(4)),
    ("extra/evidence.json:aligned-roundtrip:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object));
"###, Some(4)),
    ("extra/evidence.json:aligned-roundtrip:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object));
"###, Some(4)),
    ("extra/evidence.json:aligned-plus:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+0));
"###, Some(32)),
    ("extra/evidence.json:aligned-plus:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+0));
"###, Some(32)),
    ("extra/evidence.json:aligned-plus:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+0));
"###, Some(4)),
    ("extra/evidence.json:aligned-plus:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+0));
"###, Some(4)),
    ("extra/evidence.json:aligned-plus:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+0));
"###, Some(4)),
    ("extra/evidence.json:aligned-plus:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+0));
"###, Some(4)),
    ("extra/evidence.json:aligned-plus:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+0));
"###, Some(4)),
    ("extra/evidence.json:aligned-plus:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+0));
"###, Some(32)),
    ("extra/evidence.json:aligned-plus:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+0));
"###, Some(32)),
    ("extra/evidence.json:aligned-plus:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+0));
"###, Some(4)),
    ("extra/evidence.json:aligned-plus:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+0));
"###, Some(4)),
    ("extra/evidence.json:aligned-plus:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+0));
"###, Some(4)),
    ("extra/evidence.json:aligned-plus:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+0));
"###, Some(4)),
    ("extra/evidence.json:aligned-plus:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+0));
"###, Some(4)),
    ("extra/evidence.json:packed-cast:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=__alignof__(*((int*)&object.v));
"###, Some(1)),
    ("extra/evidence.json:packed-cast:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=__alignof__(*((int*)&object.v));
"###, Some(1)),
    ("extra/evidence.json:packed-cast:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=__alignof__(*((int*)&object.v));
"###, Some(4)),
    ("extra/evidence.json:packed-cast:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=__alignof__(*((int*)&object.v));
"###, Some(4)),
    ("extra/evidence.json:packed-cast:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=__alignof__(*((int*)&object.v));
"###, Some(4)),
    ("extra/evidence.json:packed-cast:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=__alignof__(*((int*)&object.v));
"###, Some(4)),
    ("extra/evidence.json:packed-cast:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=__alignof__(*((int*)&object.v));
"###, Some(4)),
    ("extra/evidence.json:packed-cast:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=_Alignof(*((int*)&object.v));
"###, Some(1)),
    ("extra/evidence.json:packed-cast:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=_Alignof(*((int*)&object.v));
"###, Some(1)),
    ("extra/evidence.json:packed-cast:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=_Alignof(*((int*)&object.v));
"###, Some(4)),
    ("extra/evidence.json:packed-cast:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=_Alignof(*((int*)&object.v));
"###, Some(4)),
    ("extra/evidence.json:packed-cast:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=_Alignof(*((int*)&object.v));
"###, Some(4)),
    ("extra/evidence.json:packed-cast:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=_Alignof(*((int*)&object.v));
"###, Some(4)),
    ("extra/evidence.json:packed-cast:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;
unsigned long long result=_Alignof(*((int*)&object.v));
"###, Some(4)),
    ("extra/evidence.json:aligned-conditional:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=__alignof__(*(1?&object:&plain));
"###, Some(4)),
    ("extra/evidence.json:aligned-conditional:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=__alignof__(*(1?&object:&plain));
"###, Some(4)),
    ("extra/evidence.json:aligned-conditional:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=__alignof__(*(1?&object:&plain));
"###, Some(4)),
    ("extra/evidence.json:aligned-conditional:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=__alignof__(*(1?&object:&plain));
"###, Some(4)),
    ("extra/evidence.json:aligned-conditional:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=__alignof__(*(1?&object:&plain));
"###, Some(4)),
    ("extra/evidence.json:aligned-conditional:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=__alignof__(*(1?&object:&plain));
"###, Some(4)),
    ("extra/evidence.json:aligned-conditional:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=__alignof__(*(1?&object:&plain));
"###, Some(4)),
    ("extra/evidence.json:aligned-conditional:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=_Alignof(*(1?&object:&plain));
"###, Some(4)),
    ("extra/evidence.json:aligned-conditional:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=_Alignof(*(1?&object:&plain));
"###, Some(4)),
    ("extra/evidence.json:aligned-conditional:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=_Alignof(*(1?&object:&plain));
"###, Some(4)),
    ("extra/evidence.json:aligned-conditional:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=_Alignof(*(1?&object:&plain));
"###, Some(4)),
    ("extra/evidence.json:aligned-conditional:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=_Alignof(*(1?&object:&plain));
"###, Some(4)),
    ("extra/evidence.json:aligned-conditional:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=_Alignof(*(1?&object:&plain));
"###, Some(4)),
    ("extra/evidence.json:aligned-conditional:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));int plain;
unsigned long long result=_Alignof(*(1?&object:&plain));
"###, Some(4)),
    ("extra/evidence.json:redecl-increase:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"extern int object __attribute__((aligned(8)));extern int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-increase:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"extern int object __attribute__((aligned(8)));extern int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-increase:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"extern int object __attribute__((aligned(8)));extern int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-increase:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"extern int object __attribute__((aligned(8)));extern int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-increase:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"extern int object __attribute__((aligned(8)));extern int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-increase:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"extern int object __attribute__((aligned(8)));extern int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-increase:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"extern int object __attribute__((aligned(8)));extern int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-increase:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"extern int object __attribute__((aligned(8)));extern int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-increase:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"extern int object __attribute__((aligned(8)));extern int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-increase:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"extern int object __attribute__((aligned(8)));extern int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-increase:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"extern int object __attribute__((aligned(8)));extern int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-increase:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"extern int object __attribute__((aligned(8)));extern int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-increase:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"extern int object __attribute__((aligned(8)));extern int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-increase:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"extern int object __attribute__((aligned(8)));extern int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-decrease:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"extern int object __attribute__((aligned(32)));extern int object __attribute__((aligned(8)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-decrease:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"extern int object __attribute__((aligned(32)));extern int object __attribute__((aligned(8)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-decrease:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object __attribute__((aligned(8)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-decrease:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object __attribute__((aligned(8)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-decrease:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object __attribute__((aligned(8)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-decrease:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object __attribute__((aligned(8)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-decrease:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object __attribute__((aligned(8)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-decrease:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"extern int object __attribute__((aligned(32)));extern int object __attribute__((aligned(8)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-decrease:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"extern int object __attribute__((aligned(32)));extern int object __attribute__((aligned(8)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-decrease:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object __attribute__((aligned(8)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-decrease:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object __attribute__((aligned(8)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-decrease:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object __attribute__((aligned(8)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-decrease:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object __attribute__((aligned(8)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-decrease:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object __attribute__((aligned(8)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-plain:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"extern int object __attribute__((aligned(32)));extern int object;
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-plain:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"extern int object __attribute__((aligned(32)));extern int object;
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-plain:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object;
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-plain:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object;
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-plain:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object;
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-plain:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object;
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-plain:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object;
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-plain:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"extern int object __attribute__((aligned(32)));extern int object;
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-plain:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"extern int object __attribute__((aligned(32)));extern int object;
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-plain:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object;
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-plain:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object;
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-plain:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object;
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-plain:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object;
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:redecl-plain:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"extern int object __attribute__((aligned(32)));extern int object;
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:undersized-alignas:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"_Alignas(1) int object;
unsigned long long result=__alignof__(object);
"###, None),
    ("extra/evidence.json:undersized-alignas:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"_Alignas(1) int object;
unsigned long long result=__alignof__(object);
"###, None),
    ("extra/evidence.json:undersized-alignas:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"_Alignas(1) int object;
unsigned long long result=__alignof__(object);
"###, None),
    ("extra/evidence.json:undersized-alignas:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"_Alignas(1) int object;
unsigned long long result=__alignof__(object);
"###, None),
    ("extra/evidence.json:undersized-alignas:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"_Alignas(1) int object;
unsigned long long result=__alignof__(object);
"###, None),
    ("extra/evidence.json:undersized-alignas:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"_Alignas(1) int object;
unsigned long long result=__alignof__(object);
"###, None),
    ("extra/evidence.json:undersized-alignas:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"_Alignas(1) int object;
unsigned long long result=__alignof__(object);
"###, None),
    ("extra/evidence.json:undersized-alignas:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"_Alignas(1) int object;
unsigned long long result=_Alignof(object);
"###, None),
    ("extra/evidence.json:undersized-alignas:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"_Alignas(1) int object;
unsigned long long result=_Alignof(object);
"###, None),
    ("extra/evidence.json:undersized-alignas:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"_Alignas(1) int object;
unsigned long long result=_Alignof(object);
"###, None),
    ("extra/evidence.json:undersized-alignas:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"_Alignas(1) int object;
unsigned long long result=_Alignof(object);
"###, None),
    ("extra/evidence.json:undersized-alignas:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"_Alignas(1) int object;
unsigned long long result=_Alignof(object);
"###, None),
    ("extra/evidence.json:undersized-alignas:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"_Alignas(1) int object;
unsigned long long result=_Alignof(object);
"###, None),
    ("extra/evidence.json:undersized-alignas:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"_Alignas(1) int object;
unsigned long long result=_Alignof(object);
"###, None),
    ("extra/evidence.json:extern-incomplete-aligned:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct S; extern struct S object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:extern-incomplete-aligned:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct S; extern struct S object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, Some(32)),
    ("extra/evidence.json:extern-incomplete-aligned:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct S; extern struct S object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, None),
    ("extra/evidence.json:extern-incomplete-aligned:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct S; extern struct S object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, None),
    ("extra/evidence.json:extern-incomplete-aligned:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct S; extern struct S object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, None),
    ("extra/evidence.json:extern-incomplete-aligned:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct S; extern struct S object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, None),
    ("extra/evidence.json:extern-incomplete-aligned:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct S; extern struct S object __attribute__((aligned(32)));
unsigned long long result=__alignof__(object);
"###, None),
    ("extra/evidence.json:extern-incomplete-aligned:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct S; extern struct S object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:extern-incomplete-aligned:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct S; extern struct S object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, Some(32)),
    ("extra/evidence.json:extern-incomplete-aligned:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct S; extern struct S object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, None),
    ("extra/evidence.json:extern-incomplete-aligned:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct S; extern struct S object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, None),
    ("extra/evidence.json:extern-incomplete-aligned:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct S; extern struct S object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, None),
    ("extra/evidence.json:extern-incomplete-aligned:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct S; extern struct S object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, None),
    ("extra/evidence.json:extern-incomplete-aligned:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct S; extern struct S object __attribute__((aligned(32)));
unsigned long long result=_Alignof(object);
"###, None),
    ("pointers/evidence.json:aligned-plus-one:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object[0]+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-plus-one:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object[0]+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-plus-one:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object[0]+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-plus-one:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object[0]+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-plus-one:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object[0]+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-plus-one:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object[0]+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-plus-one:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object[0]+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-plus-one:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object[0]+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-plus-one:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object[0]+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-plus-one:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object[0]+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-plus-one:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object[0]+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-plus-one:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object[0]+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-plus-one:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object[0]+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-plus-one:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object[0]+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-one:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-one:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-one:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-one:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-one:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-one:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-one:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-one:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-one:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-one:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-one:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-one:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-one:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-one:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+1));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-two:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+2));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-two:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+2));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-two:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+2));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-two:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+2));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-two:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+2));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-two:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+2));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-two:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object+2));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-two:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+2));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-two:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+2));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-two:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+2));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-two:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+2));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-two:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+2));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-two:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+2));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-two:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object+2));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-unknown:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));int i;
unsigned long long result=__alignof__(*(&object+i));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-unknown:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));int i;
unsigned long long result=__alignof__(*(&object+i));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-unknown:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));int i;
unsigned long long result=__alignof__(*(&object+i));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-unknown:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));int i;
unsigned long long result=__alignof__(*(&object+i));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-unknown:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));int i;
unsigned long long result=__alignof__(*(&object+i));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-unknown:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));int i;
unsigned long long result=__alignof__(*(&object+i));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-unknown:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));int i;
unsigned long long result=__alignof__(*(&object+i));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-unknown:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));int i;
unsigned long long result=_Alignof(*(&object+i));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-unknown:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));int i;
unsigned long long result=_Alignof(*(&object+i));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-unknown:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));int i;
unsigned long long result=_Alignof(*(&object+i));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-unknown:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));int i;
unsigned long long result=_Alignof(*(&object+i));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-unknown:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));int i;
unsigned long long result=_Alignof(*(&object+i));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-unknown:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));int i;
unsigned long long result=_Alignof(*(&object+i));
"###, Some(4)),
    ("pointers/evidence.json:aligned-direct-plus-unknown:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));int i;
unsigned long long result=_Alignof(*(&object+i));
"###, Some(4)),
    ("pointers/evidence.json:aligned-comma-pointer:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(0,&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-comma-pointer:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(0,&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-comma-pointer:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(0,&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-comma-pointer:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(0,&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-comma-pointer:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(0,&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-comma-pointer:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(0,&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-comma-pointer:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(0,&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-comma-pointer:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(0,&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-comma-pointer:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(0,&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-comma-pointer:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(0,&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-comma-pointer:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(0,&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-comma-pointer:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(0,&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-comma-pointer:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(0,&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-comma-pointer:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(0,&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-cast-void:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*((int*)(void*)&object));
"###, Some(32)),
    ("pointers/evidence.json:aligned-cast-void:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*((int*)(void*)&object));
"###, Some(32)),
    ("pointers/evidence.json:aligned-cast-void:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*((int*)(void*)&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-cast-void:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*((int*)(void*)&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-cast-void:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*((int*)(void*)&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-cast-void:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*((int*)(void*)&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-cast-void:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*((int*)(void*)&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-cast-void:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*((int*)(void*)&object));
"###, Some(32)),
    ("pointers/evidence.json:aligned-cast-void:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*((int*)(void*)&object));
"###, Some(32)),
    ("pointers/evidence.json:aligned-cast-void:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*((int*)(void*)&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-cast-void:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*((int*)(void*)&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-cast-void:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*((int*)(void*)&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-cast-void:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*((int*)(void*)&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-cast-void:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*((int*)(void*)&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-conditional-same:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(1?&object:&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-conditional-same:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(1?&object:&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-conditional-same:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(1?&object:&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-conditional-same:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(1?&object:&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-conditional-same:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(1?&object:&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-conditional-same:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(1?&object:&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-conditional-same:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(1?&object:&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-conditional-same:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(1?&object:&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-conditional-same:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(1?&object:&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-conditional-same:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(1?&object:&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-conditional-same:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(1?&object:&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-conditional-same:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(1?&object:&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-conditional-same:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(1?&object:&object));
"###, Some(4)),
    ("pointers/evidence.json:aligned-conditional-same:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(1?&object:&object));
"###, Some(4)),
    ("pointers/evidence.json:packed-pointer-unknown:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;int i;
unsigned long long result=__alignof__(*(&object.v+i));
"###, Some(4)),
    ("pointers/evidence.json:packed-pointer-unknown:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;int i;
unsigned long long result=__alignof__(*(&object.v+i));
"###, Some(4)),
    ("pointers/evidence.json:packed-pointer-unknown:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;int i;
unsigned long long result=__alignof__(*(&object.v+i));
"###, Some(4)),
    ("pointers/evidence.json:packed-pointer-unknown:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;int i;
unsigned long long result=__alignof__(*(&object.v+i));
"###, Some(4)),
    ("pointers/evidence.json:packed-pointer-unknown:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;int i;
unsigned long long result=__alignof__(*(&object.v+i));
"###, Some(4)),
    ("pointers/evidence.json:packed-pointer-unknown:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;int i;
unsigned long long result=__alignof__(*(&object.v+i));
"###, Some(4)),
    ("pointers/evidence.json:packed-pointer-unknown:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;int i;
unsigned long long result=__alignof__(*(&object.v+i));
"###, Some(4)),
    ("pointers/evidence.json:packed-pointer-unknown:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;int i;
unsigned long long result=_Alignof(*(&object.v+i));
"###, Some(4)),
    ("pointers/evidence.json:packed-pointer-unknown:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;int i;
unsigned long long result=_Alignof(*(&object.v+i));
"###, Some(4)),
    ("pointers/evidence.json:packed-pointer-unknown:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;int i;
unsigned long long result=_Alignof(*(&object.v+i));
"###, Some(4)),
    ("pointers/evidence.json:packed-pointer-unknown:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;int i;
unsigned long long result=_Alignof(*(&object.v+i));
"###, Some(4)),
    ("pointers/evidence.json:packed-pointer-unknown:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;int i;
unsigned long long result=_Alignof(*(&object.v+i));
"###, Some(4)),
    ("pointers/evidence.json:packed-pointer-unknown:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;int i;
unsigned long long result=_Alignof(*(&object.v+i));
"###, Some(4)),
    ("pointers/evidence.json:packed-pointer-unknown:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"struct __attribute__((packed)) S{char x;int v;};struct S object;int i;
unsigned long long result=_Alignof(*(&object.v+i));
"###, Some(4)),
    ("pointers/evidence.json:array-decay-deref:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("pointers/evidence.json:array-decay-deref:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("pointers/evidence.json:array-decay-deref:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("pointers/evidence.json:array-decay-deref:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("pointers/evidence.json:array-decay-deref:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("pointers/evidence.json:array-decay-deref:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("pointers/evidence.json:array-decay-deref:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*object);
"###, Some(4)),
    ("pointers/evidence.json:array-decay-deref:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("pointers/evidence.json:array-decay-deref:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("pointers/evidence.json:array-decay-deref:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("pointers/evidence.json:array-decay-deref:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("pointers/evidence.json:array-decay-deref:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("pointers/evidence.json:array-decay-deref:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("pointers/evidence.json:array-decay-deref:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*object);
"###, Some(4)),
    ("pointers/evidence.json:array-address-deref:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object));
"###, Some(32)),
    ("pointers/evidence.json:array-address-deref:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object));
"###, Some(32)),
    ("pointers/evidence.json:array-address-deref:__alignof__", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object));
"###, Some(4)),
    ("pointers/evidence.json:array-address-deref:__alignof__", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object));
"###, Some(4)),
    ("pointers/evidence.json:array-address-deref:__alignof__", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object));
"###, Some(4)),
    ("pointers/evidence.json:array-address-deref:__alignof__", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object));
"###, Some(4)),
    ("pointers/evidence.json:array-address-deref:__alignof__", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=__alignof__(*(&object));
"###, Some(4)),
    ("pointers/evidence.json:array-address-deref:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object));
"###, Some(32)),
    ("pointers/evidence.json:array-address-deref:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Gnu, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object));
"###, Some(32)),
    ("pointers/evidence.json:array-address-deref:_Alignof", Target::X86_64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object));
"###, Some(4)),
    ("pointers/evidence.json:array-address-deref:_Alignof", Target::Aarch64UnknownLinuxGnu, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object));
"###, Some(4)),
    ("pointers/evidence.json:array-address-deref:_Alignof", Target::X86_64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object));
"###, Some(4)),
    ("pointers/evidence.json:array-address-deref:_Alignof", Target::Aarch64AppleDarwin, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object));
"###, Some(4)),
    ("pointers/evidence.json:array-address-deref:_Alignof", Target::X86_64PcWindowsMsvc, Compiler::Clang, r###"int object[4] __attribute__((aligned(32)));
unsigned long long result=_Alignof(*(&object));
"###, Some(4)),
];
