struct extent_record { char bytes[10]; int middle; };
int extents(int n, char *unknown) {
    struct extent_record record;
    char array[10], matrix[3][4];
    __builtin_object_size(&record.middle, 1);
    __builtin_object_size(matrix[1], 3);
    __builtin_object_size(record.bytes + 2, 3);
    __builtin_object_size(&*(int *)array + 1, 3);
    __builtin_object_size((int (*)[n++])array, 0);
    __builtin_object_size((int (*)[n++])"literal", 0);
    __builtin_dynamic_object_size(unknown++, 0);
    return n;
}
