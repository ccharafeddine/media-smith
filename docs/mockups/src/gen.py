import os
HEAD='''<!doctype html><html><head><meta charset="utf-8"><link rel="stylesheet" href="../../../packages/ui/src/tokens.css"><link rel="stylesheet" href="editor.css"><link rel="stylesheet" href="extra.css"></head><body>'''
TOOLS=[("Edit","✂"),("Clip","▭"),("GIF","◎"),("Captions","CC"),("Design","◆"),("Import","⇣")]
def top(preset="TikTok", btn="Export"):
    chips="".join(f'<button class="chip {"on" if p==preset else ""}">{p}</button>' for p in ["TikTok","Reels","Shorts","YouTube","GIF"])
    return f'''<header class="ms-top"><div class="brand"><span class="mark">◆</span><b>Media</b><span class="smith">Smith</span></div><div class="proj">skate_edit_v3 <span class="dim">· autosaved 2 min ago</span></div><div class="presets">{chips}</div><span class="badge">desktop</span><button class="export">{btn}</button></header>'''
def tools(on):
    return '<nav class="ms-tools">'+"".join(f'<button class="tool {"on" if t==on else ""}"><span class="ico">{i}</span><span>{t}</span></button>' for t,i in TOOLS)+'</nav>'
PX=90
TR=[("caption","Captions",[(400,1800,"wait for it…"),(2400,2100,"this is the part"),(4700,2600,"nobody expected 😳"),(9000,2400,"follow for part 2")]),
("text","Text",[(0,3000,"TITLE · Bold Pop")]),
("video","Video 1",[(0,5200,"skate_intro.mp4"),(5200,6800,"trick_slowmo.mov")]),
("video","Overlay",[(6000,3000,"logo.png")]),
("audio","Audio",[(0,12000,"lofi_beat.mp3")])]
def timeline(sel="skate_intro.mp4", ph=3400):
    ruler='<div class="ruler">'+"".join(f'<span style="left:{s*PX}px">0:{s:02d}</span>' for s in range(13))+'</div>'
    rows=""
    for k,n,items in TR:
        its=""
        for st,d,l in items:
            inner=f'<div class="thumbs" style="position:absolute;inset:0;opacity:.35">{"<i></i>"*int(d/700)}</div><span style="position:relative">{l}</span>' if k=="video" else l
            its+=f'<div class="item {"sel" if l==sel else ""}" style="left:{st*PX/1000}px;width:{d*PX/1000}px">{inner}</div>'
        rows+=f'<div class="track {k}"><div class="head">{n}</div><div class="lane">{its}</div></div>'
    return f'<section class="ms-timeline">{ruler}{rows}<div class="playhead" style="left:{112+ph*PX/1000}px"></div></section>'
DEFCAP='this <span class="word hl">is</span> the part'
def stage(cap=DEFCAP, aspect="9 / 16", extra=""):
    c=f'<div class="cap">{cap}</div>' if cap else ""
    return f'''<main class="ms-stage"><div class="canvas" style="aspect-ratio:{aspect}"><div class="scene"></div><div class="person"></div>{extra}{c}</div><div class="transport"><span class="tc">0:03.4</span><button class="play">▶</button><span class="tc dim">0:12.0</span></div></main>'''
def page(name, body):
    open(f"{name}.html","w").write(HEAD+body+"</body></html>")

panel='''<aside class="ms-panel"><h3>Clip · skate_intro.mp4</h3><div class="seg"><span class="on">Basic</span><span>Speed</span><span>Animate</span><span>Color</span></div>
<div class="knob"><div class="kv">Scale<b>112%</b></div><div class="bar"><i style="width:56%"></i></div></div>
<div class="knob"><div class="kv">Opacity<b>100%</b></div><div class="bar"><i style="width:100%"></i></div></div>
<div class="knob"><div class="kv">Volume<b>80%</b></div><div class="bar"><i style="width:40%"></i></div></div>
<div class="sec">Reframe</div><div class="grid3"><div class="sw on" style="font-size:11px">Track face</div><div class="sw" style="font-size:11px">Blur fill</div><div class="sw" style="font-size:11px">Fit</div></div>
<div class="sec">Transition out</div><div class="card kv">Crossfade<b>0.4 s</b></div>
<div style="flex:1"></div><div class="btn2">Split at playhead  ·  S</div></aside>'''
page("01-editor", '<div class="ms">'+top()+tools("Edit")+stage()+panel+timeline()+'</div>')

lines=[("0:00.4","wait for it…",False),("0:02.4","this is the part",True),("0:04.7","nobody expected 😳",False),("0:07.3","…and he lands it",False),("0:09.0","follow for part 2",False)]
cl="".join(f'<div class="cline {"on" if o else ""}"><time>{t}</time><span>{x}</span></div>' for t,x,o in lines)
styles="".join(f'<div class="sw {"on" if i==1 else ""}" style="{s}">{n}</div>' for i,(n,s) in enumerate([("Classic","background:#111;color:#fff"),("Pop","background:#a974ff;color:#fff;-webkit-text-stroke:1px #000"),("Neon","background:#062020;color:#45f2f2;text-shadow:0 0 8px #45f2f2"),("Karaoke","background:#111;color:#ffb547"),("Boxed","background:#fff;color:#000"),("Minimal","background:#111;color:#ddd;font-weight:500")]))
cpanel=f'''<aside class="ms-panel" style="overflow:hidden"><h3>Captions</h3><div class="card"><div class="kv">Auto-caption<b style="color:var(--accent-2)">whisper · small · GPU</b></div><div class="kv" style="margin-top:6px">Language<b>English (auto)</b></div><div class="bar" style="margin-top:10px"><i style="width:100%"></i></div><div class="kv" style="margin-top:6px">Transcribed 0:12 in 3.1 s<b>48 words</b></div></div>
<div class="sec">Style</div><div class="grid3">{styles}</div>
<div class="grid2"><div class="card kv">Words / line<b>3</b></div><div class="card kv">Highlight<b>Word</b></div></div>
<div class="sec">Transcript</div><div style="display:flex;flex-direction:column;gap:6px">{cl}</div></aside>'''
page("02-captions", '<div class="ms">'+top("Reels","Export")+tools("Captions")+stage()+cpanel+timeline("this is the part")+'</div>')

gp='''<aside class="ms-panel"><h3>GIF export</h3><div class="seg"><span>320</span><span class="on">480</span><span>640</span><span>Source</span></div>
<div class="knob"><div class="kv">Frame rate<b>15 fps</b></div><div class="bar"><i style="width:50%"></i></div></div>
<div class="knob"><div class="kv">Colors<b>256 · diff palette</b></div><div class="bar"><i style="width:100%"></i></div></div>
<div class="sec">Dither</div><div class="seg"><span class="on">Bayer</span><span>Floyd</span><span>None</span></div>
<div class="sec">Loop</div><div class="seg"><span class="on">Forever</span><span>Once</span><span>Boomerang</span></div>
<div class="card"><div class="kv">Range<b>0:05.2 – 0:09.2 (4.0 s)</b></div><div class="kv" style="margin-top:6px">Estimated size<b style="color:var(--accent-2)">3.8 MB</b></div><div class="kv" style="margin-top:6px">Target<b>≤ 8 MB · Discord</b></div></div>
<div style="flex:1"></div><div class="btnp">Export GIF</div><div class="btn2">Copy to clipboard</div></aside>'''
extra='<div style="position:absolute;top:12px;left:12px;background:rgba(0,0,0,.6);padding:4px 10px;border-radius:999px;font-size:11px;font-weight:700;color:#45f2f2">GIF PREVIEW · 480×480 · 15 fps</div>'
page("03-gif-export", '<div class="ms">'+top("GIF","Export GIF")+tools("GIF")+stage(cap="", aspect="1 / 1", extra=extra)+gp+timeline("trick_slowmo.mov",6800)+'</div>')

tpl="".join(f'<div class="sw" style="height:96px;background:{b};font-size:12px">{n}</div>' for n,b in [("Thumbnail","linear-gradient(135deg,#ff5fa2,#a974ff)"),("Story","linear-gradient(160deg,#45f2f2,#1a2a4a)"),("Quote","#f5efe6;color:#111"),("Promo","linear-gradient(135deg,#ffb547,#ff5d5d)"),("Podcast","#141414"),("Meme","#fff;color:#000")])
dleft=f'''<aside class="ms-panel" style="grid-area:panel"><h3>Templates</h3><div class="seg"><span class="on">All</span><span>YouTube</span><span>Social</span><span>Mine</span></div><div class="grid2">{tpl}</div><div class="sec">Elements</div><div class="grid3"><div class="sw">T</div><div class="sw">■</div><div class="sw">●</div><div class="sw">★</div><div class="sw">↗</div><div class="sw">🖼</div></div><div class="sec">Brand kit</div><div style="display:flex;gap:6px">'''+"".join(f'<i style="width:28px;height:28px;border-radius:50%;background:{c};display:block"></i>' for c in ["#a974ff","#45f2f2","#ff5fa2","#ffb547","#ededed"])+'</div></aside>'
art='''<main class="ms-stage" style="grid-area:stage"><div style="display:flex;gap:10px;color:var(--fg-2);font-size:12px"><span class="chip on">1280 × 720</span><span class="chip">Zoom 62%</span><span class="chip">Snap ✓</span><span class="chip">Layers 6</span></div><div class="canvas" style="aspect-ratio:16/9;height:auto;width:880px;"><div class="scene"></div><div class="person" style="left:72%;width:36%;height:90%"></div>
<div style="position:absolute;left:6%;top:16%;font:800 58px/1 Syne;color:#fff;text-shadow:0 5px 0 #000, 0 0 2px #000;max-width:62%">I LANDED<br><span style="color:#45f2f2">THE KICKFLIP</span></div>
<div style="position:absolute;left:6%;bottom:14%;background:#ffb547;color:#000;font:800 22px Syne;padding:6px 14px;border-radius:8px;transform:rotate(-3deg)">PART 2 ▶</div>
<div style="position:absolute;left:5%;top:13%;width:58%;height:38%;border:1.5px solid #45f2f2;border-radius:2px"><i style="position:absolute;width:10px;height:10px;background:#fff;border:2px solid #45f2f2;left:-6px;top:-6px"></i><i style="position:absolute;width:10px;height:10px;background:#fff;border:2px solid #45f2f2;right:-6px;bottom:-6px"></i></div></div>
<div style="display:flex;gap:8px"><span class="btn2" style="padding:8px 16px">Font · Syne 800</span><span class="btn2" style="padding:8px 16px">Stroke 2px</span><span class="btn2" style="padding:8px 16px">Shadow</span><span class="btn2" style="padding:8px 16px">Align</span></div></main>'''
page("04-design", '<div class="ms" style="grid-template:\'top top top\' 52px \'tools stage panel\' 1fr / 76px 1fr 340px">'+top("YouTube","Export PNG")+tools("Design")+art+dleft+'</div>')

cs='''<div class="cs" style="width:1440px;height:900px;display:grid;grid-template-rows:56px 1fr 170px;background:var(--bg-deep)">
<header class="ms-top"><div class="brand" style="font-size:20px"><b>Clip</b><span class="smith">Smith</span><span class="badge" style="margin-left:10px">2.1</span></div><div class="proj"></div><div class="presets"><button class="chip on">Lossless cut</button><button class="chip">Re-encode</button><button class="chip">Reframe 9:16</button></div><button class="export">Export clip</button></header>
<div style="display:grid;grid-template-columns:1fr 320px;min-height:0"><main class="ms-stage"><div class="canvas" style="aspect-ratio:16/9;height:auto;width:900px"><div class="scene"></div><div class="person" style="width:30%"></div><div style="position:absolute;inset:0 33% 0 33%;border-left:1.5px dashed #45f2f2;border-right:1.5px dashed #45f2f2;background:rgba(69,242,242,.04)"></div><div style="position:absolute;top:12px;right:12px;background:rgba(0,0,0,.6);padding:4px 10px;border-radius:999px;font-size:11px;color:#45f2f2;font-weight:700">9:16 · face tracked</div></div><div class="transport"><span class="tc">0:41.2</span><button class="play">▶</button><span class="tc dim">3:12.8</span></div></main>
<aside class="ms-panel"><h3>Import</h3><div class="card" style="color:var(--fg-2)">https://example.com/watch/skate<div class="bar" style="margin-top:10px"><i style="width:72%"></i></div><div class="kv" style="margin-top:6px">yt-dlp · 1080p60 source<b>72%</b></div></div><div class="sec">Clip</div><div class="card kv">In<b>0:38.0</b></div><div class="card kv">Out<b>0:52.4</b></div><div class="card kv">Length<b style="color:var(--accent)">14.4 s</b></div><div class="sec">Quality</div><div class="seg"><span>Draft</span><span>Balanced</span><span class="on">High</span></div><div class="kv">Encoder<b>NVENC (auto)</b></div><div style="flex:1"></div><div class="btn2">Send to MediaSmith ↗</div></aside></div>
<section class="ms-timeline" style="padding:16px 24px"><div style="position:relative;height:70px;border-radius:10px;overflow:hidden;border:1px solid var(--border)"><div class="thumbs">'''+"<i></i>"*40+'''</div><div style="position:absolute;top:0;bottom:0;left:0;width:19.7%;background:rgba(0,0,0,.65)"></div><div style="position:absolute;top:0;bottom:0;left:27.3%;right:0;background:rgba(0,0,0,.65)"></div><div style="position:absolute;top:0;bottom:0;left:19.7%;width:7.6%;border:3px solid var(--accent);border-radius:6px;box-shadow:0 0 20px rgba(169,116,255,.6)"></div><div class="playhead" style="left:21.4%"></div></div><div class="kv" style="margin-top:10px"><span>0:00</span><span>Keyframes shown · Scenes detected: 14</span><span>3:12.8</span></div></section></div>'''
page("05-clipsmith-refresh", cs)
