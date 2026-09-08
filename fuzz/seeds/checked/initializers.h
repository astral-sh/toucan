extern int fixed[5];
int fixed[] = { 1, 2 };
unsigned short text[] = u"\U0001F600";
int use(int n) {
    int a[12] = { [2 ... 8] = n++, [4] = 9 };
    return a[3] + a[4] + n;
}
