import sys, glob, os
import numpy as np, cv2, MNN
from PIL import Image, ImageDraw, ImageFont
from bidi import get_display
DET = os.path.expanduser("~/AndroidStudioProjects/bucket/ocr/1/PP-OCRv6/PP-OCRv6_small_det_int8.mnn")
MEAN = np.array([0.485,0.456,0.406], np.float32); STD = np.array([0.229,0.224,0.225], np.float32)

def sess_of(path):
    itp = MNN.Interpreter(path); s = itp.createSession(); return itp, s, itp.getSessionInput(s)
def infer(itp, s, inp, x):
    shp = x.shape; itp.resizeTensor(inp, shp); itp.resizeSession(s)
    inp.copyFrom(MNN.Tensor(shp, MNN.Halide_Type_Float, np.ascontiguousarray(x), MNN.Tensor_DimensionType_Caffe))
    itp.runSession(s); o = itp.getSessionOutput(s); osh = tuple(o.getShape())
    ot = MNN.Tensor(osh, MNN.Halide_Type_Float, np.zeros(osh, np.float32), MNN.Tensor_DimensionType_Caffe)
    o.copyToHostTensor(ot); return np.array(ot.getData()).reshape(osh)

di, ds, dinp = sess_of(DET)
def detect(rgb):
    h0, w0 = rgb.shape[:2]
    sc = min(960/max(h0,w0), 1.0); nh = max(32,int(round(h0*sc/32))*32); nw = max(32,int(round(w0*sc/32))*32)
    im = cv2.resize(rgb,(nw,nh)).astype(np.float32)/255.0
    x = np.transpose((im-MEAN)/STD,(2,0,1))[None]
    prob = infer(di,ds,dinp,x).squeeze()
    ph,pw = prob.shape; sx,sy = w0/pw, h0/ph
    binmap = cv2.dilate((prob>0.3).astype(np.uint8), np.ones((3,3),np.uint8), 2)
    cnts,_ = cv2.findContours(binmap, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
    boxes = []
    for c in cnts:
        x,y,bw,bh = cv2.boundingRect(c)
        if bw*bh < 64 or prob[y:y+bh,x:x+bw].mean() < 0.6: continue
        box = cv2.boxPoints(cv2.minAreaRect(c)); box[:,0]*=sx; box[:,1]*=sy
        boxes.append(box)
    return boxes, float(prob.min()), float(prob.max())

def order(p):
    p = p[np.argsort(p[:,1])]; t = p[:2][np.argsort(p[:2,0])]; b = p[2:][np.argsort(p[2:,0])]
    return np.array([t[0],t[1],b[1],b[0]], np.float32)
def cropbox(rgb, box):
    b = order(box.astype(np.float32))
    W = int(max(np.linalg.norm(b[0]-b[1]), np.linalg.norm(b[3]-b[2])))
    H = int(max(np.linalg.norm(b[0]-b[3]), np.linalg.norm(b[1]-b[2])))
    if W<10 or H<8: return None
    M = cv2.getPerspectiveTransform(b, np.array([[0,0],[W,0],[W,H],[0,H]],np.float32))
    c = cv2.warpPerspective(rgb, M, (W,H))
    if H > W*1.5: c = cv2.rotate(c, cv2.ROTATE_90_COUNTERCLOCKWISE)
    return c

ri, rs, rinp = sess_of(sys.argv[1]); mode = sys.argv[2]
charlist = [""] + [l.rstrip("\n") for l in open(sys.argv[3],encoding="utf-8")] + [" "]
def rec(strip):
    h,w,_ = strip.shape; Wd = max(16,int(round(48*w/h))//8*8)
    im = np.asarray(Image.fromarray(strip).resize((Wd,48),Image.BILINEAR),np.float32)[:,:,::-1]
    a = infer(ri,rs,rinp, np.transpose((im/255-0.5)/0.5,(2,0,1))[None]); a = a[0] if a.ndim==3 else a
    if abs(float(a[0].sum())-1)>0.05: a=np.exp(a-a.max(1,keepdims=True)); a/=a.sum(1,keepdims=True)
    idx=a.argmax(1); out=[]; sc=[]; prev=-1
    for t,i in enumerate(idx):
        if i!=prev and i!=0 and i<len(charlist) and a[t,i]>=0.3: out.append(charlist[i]); sc.append(float(a[t,i]))
        prev=i
    txt="".join(out); return (get_display(txt,base_dir="R") if mode=="rtl" else txt), (np.mean(sc) if sc else 0.0)

for path in sorted(f for f in glob.glob(sys.argv[4]+"/*") if f.lower().endswith((".jpg",".png")) and "_pred" not in f and "strip" not in f):
    rgb = np.asarray(Image.open(path).convert("RGB"))
    boxes, pmin, pmax = detect(rgb)
    strips = [(order(b)[0][1], order(b)[0][0], cropbox(rgb,b)) for b in boxes]
    strips = [s for s in strips if s[2] is not None]
    strips.sort(key=lambda s:(round(s[0]/20), s[1]))     # reading order: top->bottom, left->right
    name = os.path.splitext(os.path.basename(path))[0]
    outdir = os.path.dirname(path)
    tsv = open(os.path.join(outdir, name+"_rec.tsv"),"w")
    cells=[]; font=ImageFont.load_default(size=22)
    for i,(_,_,strip) in enumerate(strips):
        txt,score = rec(strip)
        tsv.write(f"{i}\t{score:.2f}\t{txt}\n")
        sh = Image.fromarray(strip); sh = sh.resize((min(600,sh.width*48//sh.height),48)) if sh.height else sh
        cell = Image.new("RGB",(660, 56),(255,255,255)); d=ImageDraw.Draw(cell)
        d.text((4,16),f"{i:02d}",fill=(200,0,0),font=font); cell.paste(sh,(54,4))
        cells.append(np.asarray(cell))
    tsv.close()
    if cells:
        Image.fromarray(np.concatenate(cells,0)).save(os.path.join(outdir, name+"_strips.png"))
    print(f"{name}: {len(strips)} strips  (prob {pmin:.2f}..{pmax:.2f})")
