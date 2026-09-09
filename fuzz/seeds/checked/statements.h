int use(int n) {
    int total = 0;
    for (int i = 0; i < n; i++) {
        if (i == 2) continue;
        switch (i) { case 4: goto done; default: total += i; break; }
    }
done:
    return total + ({ int x = n; x + 1; });
}
