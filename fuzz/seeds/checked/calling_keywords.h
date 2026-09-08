typedef int __attribute__((ms_abi)) Microsoft(int);
Microsoft *__cdecl factory(int);
int (__attribute__((ms_abi)) *microsoft_factory(int))(int);
int invoke(int (*__cdecl callback)(int), int value) {
    return callback(value) + 7;
}
typedef int (__cdecl *Plain)(int);
Plain choose(Plain callback) { return callback; }
