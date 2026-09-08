void stop(int);
void before(void) { stop(1); }
_Noreturn void stop(int);
void after(void) { stop(2); }
void callback(void (*f)(int)) { f(3); }
