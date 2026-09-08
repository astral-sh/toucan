__declspec(dllimport) int value;
__declspec(dllimport) inline int read_value(void) { return value; }
__declspec(dllexport) int exported;
__declspec(dllimport) typedef int Ignored;
