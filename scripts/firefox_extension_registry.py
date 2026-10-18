"""Exclusive, reversible Windows native-host registration for opt-in checks."""
import contextlib
import ctypes
from ctypes import wintypes
import msvcrt
import pathlib
import tempfile
import winreg


@contextlib.contextmanager
def temporary_native_host(manifest):
    manifest = str(pathlib.Path(manifest).resolve(strict=True))
    registry = r"Software\Mozilla\NativeMessagingHosts\com.nodecloak.fingerprint"
    lock_path = pathlib.Path(tempfile.gettempdir()) / "nodecloak-firefox-extension-check.lock"
    lock = lock_path.open("a+b")
    owned = False
    locked = False
    try:
        if lock.seek(0, 2) == 0:
            lock.write(b"\0")
            lock.flush()
        lock.seek(0)
        try:
            msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
            locked = True
        except OSError as error:
            raise RuntimeError("Another Firefox extension check owns the test registration") from error

        # Firefox searches per-user/machine registrations and both Windows views.
        for hive in (winreg.HKEY_CURRENT_USER, winreg.HKEY_LOCAL_MACHINE):
            for view in (winreg.KEY_WOW64_64KEY, winreg.KEY_WOW64_32KEY):
                try:
                    with winreg.OpenKey(hive, registry, 0, winreg.KEY_QUERY_VALUE | view):
                        raise RuntimeError("A Firefox native-host registration already exists; refusing to overwrite it")
                except FileNotFoundError:
                    pass

        # RegCreateKeyEx reports atomic ownership, unlike winreg.CreateKey.
        create = ctypes.WinDLL("advapi32", use_last_error=True).RegCreateKeyExW
        create.argtypes = [wintypes.HKEY, wintypes.LPCWSTR, wintypes.DWORD,
                           wintypes.LPWSTR, wintypes.DWORD, wintypes.DWORD,
                           ctypes.c_void_p, ctypes.POINTER(wintypes.HKEY),
                           ctypes.POINTER(wintypes.DWORD)]
        create.restype = wintypes.LONG
        handle, disposition = wintypes.HKEY(), wintypes.DWORD()
        code = create(wintypes.HKEY(int(winreg.HKEY_CURRENT_USER)), registry,
                      0, None, 0, winreg.KEY_SET_VALUE | winreg.KEY_QUERY_VALUE |
                      winreg.KEY_WOW64_64KEY, None, ctypes.byref(handle),
                      ctypes.byref(disposition))
        if code:
            raise ctypes.WinError(code)
        try:
            if disposition.value != 1:
                raise RuntimeError("A concurrent process registered the Firefox host; refusing to overwrite it")
            owned = True
            winreg.SetValueEx(int(handle.value), None, 0, winreg.REG_SZ, manifest)
        finally:
            winreg.CloseKey(int(handle.value))
        yield
    finally:
        try:
            if owned:
                try:
                    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, registry, 0,
                                        winreg.KEY_QUERY_VALUE | winreg.KEY_WOW64_64KEY) as key:
                        value, kind = winreg.QueryValueEx(key, None)
                    # Never remove a replacement installed while the check ran.
                    if kind == winreg.REG_SZ and value == manifest:
                        winreg.DeleteKeyEx(winreg.HKEY_CURRENT_USER, registry,
                                           winreg.KEY_WOW64_64KEY, 0)
                except FileNotFoundError:
                    pass
        finally:
            if locked:
                lock.seek(0)
                msvcrt.locking(lock.fileno(), msvcrt.LK_UNLCK, 1)
            lock.close()
