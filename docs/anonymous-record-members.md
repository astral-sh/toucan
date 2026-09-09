# Anonymous record members

A directly written unnamed struct or union declares an anonymous member. A
`typeof` specifier without a declarator does not. For example:

```c
struct { int member; } source;
struct Owner { __typeof__(source); int field; };
```

GCC and Clang ignore the `typeof` declaration. `Owner` has one field at offset
zero and has the size of `int`. Toucan preserves that layout instead of adding
storage for `source`'s type. Direct anonymous structs and unions retain their
member storage and promoted field names.

The [native evidence](../corpus/evidence/anonymous-typeof-member-2026-09-09.json.gz)
compares GCC and Clang and checks all Clang physical targets. Named or typedef
anonymous members admitted by Microsoft extensions are a separate target
conformance boundary.
