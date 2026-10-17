"""PyInstaller giriş noktası (OsiSub.exe ve OsiSub-konsol.exe)."""
import multiprocessing
import sys

if __name__ == "__main__":
    multiprocessing.freeze_support()
    from osisub.__main__ import main
    sys.exit(main())
