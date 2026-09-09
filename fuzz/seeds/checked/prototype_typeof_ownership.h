typedef int (*Callback)(int length, int values[static length]);
int invoke(int count, int input[static restrict count],
           int (*callback)(int size, int rows[static size],
                           int (*nested)(int width, int cells[static width])));
int ownership(int count, int values[static count], Callback callback) {
    typedef int Row[count];
    Row storage;
    int (*pointer)[count] = &storage;
    typeof(pointer++) next = pointer;
    typeof(Row) local;
    enum { pointer_size = sizeof(int (*)[count++]) };
    return callback(count, values) + sizeof *next + sizeof local + pointer_size;
}
