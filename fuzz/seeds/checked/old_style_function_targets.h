int inspect(values) __attribute__((returns_twice, target("no-mmx")))
    int values[sizeof((__builtin_ia32_emms(), 3))];
{
    return values[0];
}
