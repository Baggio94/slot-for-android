"""One sheet per theme: before (3.1.0) above, after below; the theme's
colour then the Theme wallpaper, Scroll and Grid each."""
import sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont
root = Path(sys.argv[1]); out = Path(sys.argv[2]); out.mkdir(parents=True, exist_ok=True)
font = ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf', 20)
cols = [('none', 'scroll'), ('none', 'grid'), ('theme', 'scroll'), ('theme', 'grid')]
label = {'none': 'No wallpaper', 'theme': 'Theme wallpaper'}
themes = sorted({p.name.split('_')[0] for p in (root / 'after').glob('*.png')})
for theme in themes:
    rows = [(when, [Image.open(root / when / f'{theme}_{p}_{s}.png').convert('RGB') for p, s in cols])
            for when in ('before', 'after')]
    w, h = rows[0][1][0].size
    k = min(1.0, 480 / w)
    tw, th = int(w * k), int(h * k)
    sheet = Image.new('RGB', (4 * tw + 50, 2 * (th + 34) + 10), (40, 40, 44))
    d = ImageDraw.Draw(sheet)
    for r, (when, ims) in enumerate(rows):
        for c, im in enumerate(ims):
            x, y = 10 + c * (tw + 10), 10 + r * (th + 34)
            p, s = cols[c]
            d.text((x, y), f'{("3.1.0", "3.1.1")[r]} · {label[p]} · {s.title()}', fill='white', font=font)
            sheet.paste(im.resize((tw, th), Image.LANCZOS), (x, y + 26))
    sheet.save(out / f'{theme}.png')
    print(out / f'{theme}.png')
