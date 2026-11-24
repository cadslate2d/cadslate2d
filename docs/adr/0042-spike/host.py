# What the host does to an image (ADR-0042 §3): at most 2,048 pixels on its
# long side, re-encoded as opaque 8-bit PNG (line art) or JPEG (photos).
import sys
from PIL import Image
src, dst = sys.argv[1], sys.argv[2]
img = Image.open(src)
img.load()
if img.mode in ("RGBA", "LA", "P"):
    base = Image.new("RGB", img.size, "white")
    base.paste(img.convert("RGBA"), mask=img.convert("RGBA").split()[-1])
    img = base
scale = 2048 / max(img.size)
if scale < 1:
    img = img.resize((round(img.width * scale), round(img.height * scale)), Image.LANCZOS)
if dst.endswith(".jpg"):
    img.convert("RGB").save(dst, "JPEG", quality=90)
else:
    img.convert("L" if img.mode in ("L", "1") else "RGB").save(dst, "PNG", optimize=True)
print(dst.rsplit("/", 1)[-1], img.size)
