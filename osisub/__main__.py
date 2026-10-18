import sys

if len(sys.argv) > 1 and sys.argv[1] in ("convert", "macros", "run", "check", "shift", "fix", "stats", "--version"):
    from .cli import main
    sys.exit(main())
from .gui.app import main
sys.exit(main())
