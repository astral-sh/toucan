int checkpoint(void) __attribute__((returns_twice));
int checkpoint(void);
int late(void);
int call(void) {
    int late(void) __attribute__((returns_twice));
    return late() + checkpoint();
}
