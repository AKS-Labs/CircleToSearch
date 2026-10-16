import sys, glob, os
import numpy as np, MNN
from PIL import Image
from bidi import get_display

def make_run(model, keys):
    charlist = [""] + [l.rstrip("\n") for l in open(keys, encoding="utf-8")] + [" "]
    itp = MNN.Interpreter(model); sess = itp.createSession(); inp = itp.getSessionInput(sess)
    def run(strip):
        h, w, _ = strip.shape; W = max(16, int(round(48*w/h))//8*8)
        im = Image.fromarray(strip).resize((W, 48), Image.BILINEAR)
        arr = np.asarray(im, "float32")[:, :, ::-1]
        x = np.ascontiguousarray(np.transpose((arr/255-0.5)/0.5, (2,0,1))[None])
        shp = (1,3,48,W); itp.resizeTensor(inp, shp); itp.resizeSession(sess)
        inp.copyFrom(MNN.Tensor(shp, MNN.Halide_Type_Float, x, MNN.Tensor_DimensionType_Caffe))
        itp.runSession(sess); o = itp.getSessionOutput(sess); osh = tuple(o.getShape())
        ot = MNN.Tensor(osh, MNN.Halide_Type_Float, np.zeros(osh,"float32"), MNN.Tensor_DimensionType_Caffe)
        o.copyToHostTensor(ot); a = np.array(ot.getData()).reshape(osh); a = a[0] if a.ndim==3 else a
        if abs(float(a[0].sum())-1.0) > 0.05:
            a = np.exp(a-a.max(1,keepdims=True)); a/=a.sum(1,keepdims=True)
        idx = a.argmax(1); out=[]; sc=[]; prev=-1
        for t,i in enumerate(idx):
            if i!=prev and i!=0 and i<len(charlist):
                p=float(a[t,i])
                if p>=0.3: out.append(charlist[i]); sc.append(p)
            prev=i
        return "".join(out), (float(np.mean(sc)) if sc else 0.0)
    return run

model, keys, mode, d = sys.argv[1:5]
run = make_run(model, keys)
files = sorted(f for f in glob.glob(d+"/*") if f.lower().endswith((".jpg",".png")) and "_pred" not in f)
for path in files:
    img = Image.open(path).convert("RGB"); rgb = np.asarray(img); gray = np.asarray(img.convert("L"))
    g = 255-gray if gray.mean()<127 else gray
    ink = (g < g.mean()-0.10*255).sum(1).astype("float32")
    on = ink > ink.max()*0.18; bands=[]; s=None
    for i,v in enumerate(list(on)+[False]):
        if v and s is None: s=i
        elif not v and s is not None:
            if i-s>=8: bands.append((s,i))
            s=None
    print(f"\n## {os.path.basename(path)}  [{rgb.shape[1]}x{rgb.shape[0]}, {len(bands)} lines]")
    for a,b in bands[:14]:
        pad=max(2,(b-a)//6); row=g[max(0,a-pad):b+pad]
        cols=np.where((row<row.mean()-0.10*255).any(axis=0))[0]
        if cols.size<4: continue
        x0,x1=max(0,cols[0]-pad),min(rgb.shape[1],cols[-1]+pad)
        crop=rgb[max(0,a-pad):b+pad,x0:x1]
        pred,score=run(crop)
        if not pred.strip(): continue
        disp = get_display(pred, base_dir="R") if mode=="rtl" else pred
        print(f"   [{score:.2f}] {disp}")
