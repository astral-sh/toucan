union Source { int integer; float floating; };
typedef union Source Argument __attribute__((transparent_union));
int accept(Argument);
int call(float value) { return accept(value); }
int field(Argument value) { return value.integer; }
typedef union { int *pointer; void *opaque; } Pointer __attribute__((transparent_union));
int indirect(int (*callback)(Pointer), int *value) { return callback(value); }
