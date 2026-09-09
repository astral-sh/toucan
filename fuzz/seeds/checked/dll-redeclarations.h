__declspec(dllimport) extern int data;
extern int data;
static int *address = &data;
__declspec(dllimport) __attribute__((weak)) __inline__ int callback(void) { return 1; }
void shadow(int callback) { { extern int callback(void); } }
int (*function_address)(void) = callback;
void *__builtin_malloc(unsigned long long);
void use(int n) { (void)sizeof(sizeof(int[(__builtin_malloc(1), n)])); }
__declspec(dllexport) void *__builtin_malloc(unsigned long long);
