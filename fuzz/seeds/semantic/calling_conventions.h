typedef int (__attribute__((ms_abi)) *WinCallback)(int);
typedef int (__attribute__((sysv_abi)) *SysvCallback)(int);
int __attribute__((ms_abi)) invoke(WinCallback callback, int value) {
    return callback(value);
}
struct Callbacks { WinCallback win; SysvCallback sysv; };
