extern int weak_object __attribute__((weak));
int *identity=&weak_object ?: 0;
int strong_object;
int *rejected=&weak_object ?: &strong_object;
