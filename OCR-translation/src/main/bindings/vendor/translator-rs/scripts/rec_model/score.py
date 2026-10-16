import sys, json, glob, os
import numpy as np, MNN
from PIL import Image
from bidi import get_display
def lev(a,b):
    m,n=len(a),len(b)
    if m==0 or n==0: return max(m,n)
    d=list(range(n+1))
    for i in range(1,m+1):
        prev=d[0]; d[0]=i
        for j in range(1,n+1):
            t=d[j]; d[j]=min(d[j]+1,d[j-1]+1,prev+(a[i-1]!=b[j-1])); prev=t
    return d[n]
def make_run(model,keys):
    cl=[""]+[l.rstrip("\n") for l in open(keys,encoding="utf-8")]+[" "]
    itp=MNN.Interpreter(model); s=itp.createSession(); inp=itp.getSessionInput(s)
    def run(strip):
        h,w,_=strip.shape; W=max(16,int(round(48*w/h))//8*8)
        im=np.asarray(Image.fromarray(strip).resize((W,48),Image.BILINEAR),np.float32)[:,:,::-1]
        x=np.ascontiguousarray(np.transpose((im/255-0.5)/0.5,(2,0,1))[None])
        shp=(1,3,48,W); itp.resizeTensor(inp,shp); itp.resizeSession(s)
        inp.copyFrom(MNN.Tensor(shp,MNN.Halide_Type_Float,x,MNN.Tensor_DimensionType_Caffe))
        itp.runSession(s); o=itp.getSessionOutput(s); osh=tuple(o.getShape())
        ot=MNN.Tensor(osh,MNN.Halide_Type_Float,np.zeros(osh,np.float32),MNN.Tensor_DimensionType_Caffe)
        o.copyToHostTensor(ot); a=np.array(ot.getData()).reshape(osh); a=a[0] if a.ndim==3 else a
        if abs(float(a[0].sum())-1)>0.05: a=np.exp(a-a.max(1,keepdims=True)); a/=a.sum(1,keepdims=True)
        idx=a.argmax(1); out=[]; prev=-1
        for t,i in enumerate(idx):
            if i!=prev and i!=0 and i<len(cl) and a[t,i]>=0.3: out.append(cl[i])
            prev=i
        return "".join(out)
    return run
model,mode,keys,d=sys.argv[1:5]
run=make_run(model,keys)
te=tl=we=wl=0
for gp in sorted(glob.glob(d+"/*.json")):
    name=os.path.splitext(os.path.basename(gp))[0]; gt=json.load(open(gp,encoding="utf-8"))
    for idx in sorted(gt):
        sp=f"{d}/strips/{name}-{idx}.jpg"
        if not os.path.exists(sp): continue
        pred=run(np.asarray(Image.open(sp).convert("RGB")))
        if mode=="rtl": pred=get_display(pred,base_dir="R")
        g=gt[idx]
        te+=lev(pred,g); tl+=len(g)
        we+=lev(pred.split(),g.split()); wl+=len(g.split())
print(f"{os.path.basename(d):10} CER {te/max(tl,1):.3f}  WER {we/max(wl,1):.3f}  ({tl} chars, {wl} words)")
