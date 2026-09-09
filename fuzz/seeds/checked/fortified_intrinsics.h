void copy(char *destination, const char *source, unsigned long size) {
    __builtin___memcpy_chk(destination, source, size, __builtin_object_size(destination, 0));
}
int format(char *destination, short value, float fraction) {
    return __builtin___snprintf_chk(destination, 32, 0, 32, "%d %.1f", value, fraction);
}
