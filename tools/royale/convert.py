# Convert crt-royale-fast slang passes to GLSL ES 3.00 (WebGL2 / GLES3) vertex+fragment pairs.
import re, sys, json, os
SRC = sys.argv[1]; OUT = sys.argv[2]
PASSES = ["crt-royale-first-pass-linearize-crt-gamma-bob-fields","crt-royale-scanlines-vertical-interlacing",
 "crt-royale-mask-resize-vertical","crt-royale-mask-resize-horizontal","crt-royale-scanlines-horizontal-apply-mask",
 "crt-royale-brightpass","crt-royale-bloom-vertical","crt-royale-bloom-horizontal-reconstitute"]
def inc(path, seen):
    txt = open(path).read()
    def r(m):
        p = os.path.join(os.path.dirname(path), m.group(1))
        return inc(p, seen)
    return re.sub(r'#include\s+"([^"]+)"', r, txt)
params = {}
def block(txt, pat, name, inst):
    m = re.search(pat, txt, re.S)
    if not m: return txt
    body = m.group(1)
    body = re.sub(r'\buint\s+FrameCount', 'float FrameCount', body)
    body = re.sub(r'(?m)^\s*//.*$', '', body)
    return txt[:m.start()] + f'struct {name}T {{{body}}};\nuniform {name}T {inst};' + txt[m.end():]
DECL = re.compile(r'(?:const\s+)?(?:static\s+)?(float|vec2|vec3|vec4|bool|int|uint)\s+(\w+)\s*=\s*(.*)', re.S)
def globals_to_defines(t):
    # GLSL ES: global initializers must be constant; turn every initialised
    # global into a macro (none of them is written to later).
    out = []; i = 0; depth = 0; n = len(t); stmt_start = 0
    lines = t.split('\n'); res = []; depth = 0; buf = None
    for line in lines:
        st = line.strip()
        if buf is not None:
            buf += '\n' + line
            if ';' in line:
                m = DECL.match(buf.strip()); body = m.group(3).rsplit(';', 1)[0]
                res.append('#define %s (%s(%s))' % (m.group(2), m.group(1), ' '.join(body.split())))
                buf = None
            continue
        if depth == 0 and not st.startswith('#') and DECL.match(st) and '(' not in st.split('=')[0]:
            if ';' in st:
                m = DECL.match(st); body = m.group(3).rsplit(';', 1)[0]
                res.append('#define %s (%s(%s))' % (m.group(2), m.group(1), ' '.join(body.split())))
            else:
                buf = st
            continue
        code = '' if st.startswith('#') else line.split('//')[0]
        depth += code.count('{') - code.count('}')
        res.append(line)
    return '\n'.join(res)
# GLSL 4.50 converts int to float implicitly, GLSL ES does not.
PATCHES = [
    ('input_tiles_per_texture_r, samples, ', 'input_tiles_per_texture_r, float(samples), '),
]
out = []
for p in PASSES:
    t = inc(os.path.join(SRC, p + '.slang'), set())
    t = t.replace('#version 450', '')
    for m in re.finditer(r'#pragma parameter\s+(\w+)\s+"[^"]*"\s+([-\d.eE]+)', t):
        params[m.group(1)] = float(m.group(2))
    t = re.sub(r'#pragma (parameter|format|name)[^\n]*', '', t)
    t = block(t, r'layout\(push_constant\)\s*uniform\s+Push\s*\{(.*?)\}\s*params\s*;', 'Push', 'params')
    t = block(t, r'layout\(std140,\s*set\s*=\s*0,\s*binding\s*=\s*0\)\s*uniform\s+UBO\s*\{(.*?)\}\s*global\s*;', 'UBO', 'global')
    t = re.sub(r'layout\(set\s*=\s*0,\s*binding\s*=\s*\d+\)\s*uniform', 'uniform', t)
    t = re.sub(r'layout\(location\s*=\s*\d+\)\s*', '', t)
    t = re.sub(r'/\*.*?\*/', '', t, flags=re.S)
    t = re.sub(r'\\[ \t]*\n', ' ', t)
    t = re.sub(r'//[^\n]*', '', t)
    t = re.sub(r'(?m)^(\s+)const\s+(?=(float|vec\d|bool|int|uint|mat\d)\s+\w+\s*=)', r'\1', t)
    t = globals_to_defines(t)
    for a, b in PATCHES:
        t = t.replace(a, b)
    out.append((p, t))
# Parameters become constants at their preset defaults (slot. has no
# parameter menu), so the compiler folds them.
for i, (p, t) in enumerate(out):
    for name, d in sorted(params.items(), key=lambda kv: -len(kv[0])):
        lit = repr(float(d))
        t = re.sub(r'\b(global|params)\.' + name + r'\b', '(' + lit + ')', t)
    t = t.replace('global.MVP * Position', 'Position')
    m = re.search(r'struct UBOT \{.*?\};\nuniform UBOT global;', t, re.S)
    t = t[:m.start()] + t[m.end():]
    m = re.search(r'struct PushT \{(.*?)\};\nuniform PushT params;', t, re.S)
    members = [l.strip() for l in m.group(1).split(';') if l.strip()]
    keep = [mm for mm in members if mm.split()[1] not in params]
    t = t[:m.start()] + '\n'.join('uniform %s;' % mm for mm in keep) + t[m.end():]
    t = re.sub(r'\bparams\.', '', t)
    assert 'global.' not in t, (p, re.findall(r'global\.\w+', t)[:5])
    common, rest = t.split('#pragma stage vertex', 1)
    vert, frag = rest.split('#pragma stage fragment', 1)
    # slot.'s quad: a_pos from 0 to 1, at location 0.
    assert 'in vec4 Position;' in vert and 'in vec2 TexCoord;' in vert
    vert = vert.replace('in vec4 Position;', 'in vec2 a_pos;\n#define Position vec4(a_pos * 2.0 - 1.0, 0.0, 1.0)')
    vert = vert.replace('in vec2 TexCoord;', '#define TexCoord a_pos')
    body = common + '\n#ifdef VERTEX\n' + vert + '\n#else\n' + frag + '\n#endif\n'
    body = '\n'.join(l.rstrip() for l in body.split('\n'))
    body = re.sub(r'\n{3,}', '\n\n', body)
    hdr = ('// crt-royale-fast pass %d: %s.slang\n'
           '// crt-royale (C) 2014 TroggleMonkey, crt-royale-fast by Hyllian (2024), GPL-2.0-or-later.\n'
           '// From libretro/slang-shaders crt/shaders/crt-royale/src-fast, converted to GLSL ES 3.00\n'
           '// by tools/royale/convert.py (parameters fixed at their preset defaults).\n' % (i, p))
    open(os.path.join(OUT, 'pass%d.glsl' % i), 'w').write(hdr + body)
print(len(out), 'passes', len(params), 'params')
