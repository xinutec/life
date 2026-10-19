import os, sys; sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from emocoords import W
COL={'Happy':'#d99b00','Sad':'#3b6fd4','Disgusted':'#2f9e63','Angry':'#d0392b',
     'Fearful':'#e0741a','Bad':'#1f9aa0','Surprised':'#8d5bd6','Neutral':'#8a8f98'}
pts=[]
for core,(c,groups) in W.items():
    for g,(gc,leaves) in groups.items():
        pts.append((core,g,'group',gc))
        for l,lc in leaves.items(): pts.append((core,l,'leaf',lc))
AX={'V':(0,'valence  unpleasant → pleasant'),'L':(1,'load  free → costly to hold'),'D':(2,'direction  inward → about a person')}
def panel(ox,oy,w,h,xa,ya,title):
    xi,xl=AX[xa]; yi,yl=AX[ya]
    xr=(-10,10) if xa!='L' else (0,10)
    yr=(-10,10) if ya!='L' else (0,10)
    s=[f'<g transform="translate({ox},{oy})">']
    s.append(f'<rect width="{w}" height="{h}" fill="#fbfbf9" stroke="#ddd"/>')
    s.append(f'<text x="10" y="20" font-size="15" font-weight="600" fill="#222">{title}</text>')
    def X(v): return 46+(v-xr[0])/(xr[1]-xr[0])*(w-70)
    def Y(v): return h-44-(v-yr[0])/(yr[1]-yr[0])*(h-80)
    for t in range(xr[0],xr[1]+1,5):
        s.append(f'<line x1="{X(t)}" y1="{Y(yr[0])}" x2="{X(t)}" y2="{Y(yr[1])}" stroke="#eee"/>')
        s.append(f'<text x="{X(t)}" y="{h-26}" font-size="9" fill="#999" text-anchor="middle">{t}</text>')
    for t in range(yr[0],yr[1]+1,5):
        s.append(f'<line x1="{X(xr[0])}" y1="{Y(t)}" x2="{X(xr[1])}" y2="{Y(t)}" stroke="#eee"/>')
        s.append(f'<text x="38" y="{Y(t)+3}" font-size="9" fill="#999" text-anchor="end">{t}</text>')
    s.append(f'<text x="{w/2}" y="{h-9}" font-size="11" fill="#666" text-anchor="middle">{xl}</text>')
    s.append(f'<text transform="translate(14,{h/2}) rotate(-90)" font-size="11" fill="#666" text-anchor="middle">{yl}</text>')
    # jitter identical points so overlaps are visible
    seen={}
    for core,name,kind,c in pts:
        x,y=X(c[xi]),Y(c[yi]); k=(round(x),round(y))
        n=seen.get(k,0); seen[k]=n+1
        x+=(n%4)*7; y+=(n//4)*9
        col=COL[core]
        if kind=='group':
            s.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="4" fill="none" stroke="{col}" stroke-width="1.6"/>')
        else:
            s.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="2.6" fill="{col}"/>')
            s.append(f'<text x="{x+4.5:.1f}" y="{y+3:.1f}" font-size="7.4" fill="#333">{name}</text>')
    s.append('</g>')
    return '\n'.join(s)
Wd,Ht=900,760
svg=[f'<svg xmlns="http://www.w3.org/2000/svg" width="{Wd*2+30}" height="{Ht*2+60}" font-family="-apple-system,Helvetica,sans-serif">',
     f'<rect width="100%" height="100%" fill="#fff"/>',
     f'<text x="20" y="28" font-size="18" font-weight="700">Emotion space — 219 tokens, three axes (filled = leaf, ring = group)</text>']
svg.append(panel(15,45,Wd,Ht,'L','V','load × valence'))
svg.append(panel(Wd+30,45,Wd,Ht,'D','V','direction × valence'))
svg.append(panel(15,Ht+60,Wd,Ht,'D','L','direction × load'))
lx=Wd+60
svg.append(f'<g transform="translate({Wd+45},{Ht+90})">')
for i,(k,v) in enumerate(COL.items()):
    svg.append(f'<circle cx="10" cy="{i*26+10}" r="6" fill="{v}"/><text x="26" y="{i*26+14}" font-size="14" fill="#333">{k}</text>')
svg.append('</g>')
svg.append('</svg>')
open('/tmp/emospace.svg','w').write('\n'.join(svg))
open('/tmp/emospace.html','w').write('<body style="margin:0">'+('\n'.join(svg))+'</body>')
print("wrote /tmp/emospace.html")
