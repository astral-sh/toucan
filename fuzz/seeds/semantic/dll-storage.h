__declspec(dllimport) int imported;
__declspec(dllimport) inline int read_import(void) { return imported; }
int scope(void) {
    extern int imported;
    { static int imported; static int *address = &imported; return *address; }
}
int imported;
extern int later;
int query(void) { return sizeof(later); }
__declspec(dllexport) int later;
