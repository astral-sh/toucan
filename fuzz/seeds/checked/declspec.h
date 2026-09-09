__declspec(align(16)) struct Record { int x; } object;
__declspec(align(16)) typedef int Aligned;
struct Fields { char prefix; Aligned value; };
__declspec(noreturn) typedef void (*Stop)(int);
__declspec(noinline) int read_value(struct Record *record) { return record->x; }
__declspec("SAL"(input)) int annotated(int input);
