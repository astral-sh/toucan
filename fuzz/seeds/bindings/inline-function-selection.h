int ordinary(int value) { return value + 1; }
int late_inline(int value) { return value + 3; }
inline int late_inline(int value);
static inline int local_inline(int value) { return value; }
extern inline __attribute__((gnu_inline)) int replaced(int value) { return value; }
int replaced(int value) { return value + 2; }
