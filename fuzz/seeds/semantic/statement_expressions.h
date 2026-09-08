int twice(int x) { return ({ int y = x + 1; y * 2; }); }
int jump(int n) { int a[n]; return ({ goto done; 1; }); done: return sizeof a > 0; }
