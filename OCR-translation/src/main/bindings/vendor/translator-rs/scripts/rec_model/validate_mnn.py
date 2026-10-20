"""Quick MNN-runtime check for the Hebrew recognizer: greedy CTC decode on val images.

Replicates the PaddleOCR rec preprocessing (BGR, resize-to-h48 keep ratio + pad to
320, normalize (x/255-0.5)/0.5) and the CTC class layout (blank at 0, dict at
1..N, space last). Reports exact-match and char error rate.

  python validate_mnn.py <model.mnn> <keys.txt> <val_list.txt> <data_dir> [n]
"""

import difflib
import random
import sys

import cv2
import MNN
import numpy as np

model, keys_path, val_list, data_dir = sys.argv[1:5]
n = int(sys.argv[5]) if len(sys.argv) > 5 else 50

keys = [k for k in open(keys_path, encoding="utf-8").read().split("\n")]
if keys and keys[-1] == "":
    keys = keys[:-1]
charlist = [""] + keys + [" "]  # blank, dict, space

itp = MNN.Interpreter(model)
sess = itp.createSession()
inp = itp.getSessionInput(sess)
SHP = (1, 3, 48, 320)


def prep(path):
    img = cv2.imread(path)
    h, w = img.shape[:2]
    rw = min(320, max(1, int(np.ceil(48 * w / h))))
    img = (cv2.resize(img, (rw, 48)).astype("float32") / 255.0 - 0.5) / 0.5
    c = np.zeros((48, 320, 3), "float32")
    c[:, :rw] = img
    return np.ascontiguousarray(np.transpose(c, (2, 0, 1))[None])


def decode(a):
    idx = a.argmax(1)
    out = []
    prev = -1
    for i in idx:
        if i != prev and i != 0:
            out.append(charlist[i])
        prev = i
    return "".join(out)


lines = list(open(val_list, encoding="utf-8"))
random.seed(0)
random.shuffle(lines)
lines = lines[:n]
exact = num = den = 0
examples = []
for ln in lines:
    rel, lab = ln.rstrip("\n").split("\t")
    x = prep(f"{data_dir}/{rel}")
    itp.resizeTensor(inp, SHP)
    itp.resizeSession(sess)
    inp.copyFrom(MNN.Tensor(SHP, MNN.Halide_Type_Float, x.copy(), MNN.Tensor_DimensionType_Caffe))
    itp.runSession(sess)
    o = itp.getSessionOutput(sess)
    shp = tuple(o.getShape())
    ot = MNN.Tensor(shp, MNN.Halide_Type_Float, np.zeros(shp, "float32"), MNN.Tensor_DimensionType_Caffe)
    o.copyToHostTensor(ot)
    arr = np.array(ot.getData()).reshape(shp)
    arr = arr[0] if arr.ndim == 3 else arr
    pred = decode(arr)
    # NFC + strip ZW so composed/decomposed matras and chillu (atomic vs base+ZWJ)
    # count as equal. NFC composes the Bengali two-part o/au matras, but Gujarati ો/ૌ
    # (0ACB/0ACC) have NO canonical decomposition, so fold ે+ા->ો, ૈ+ા->ૌ by hand.
    # Malayalam atomic chillu (0D7A-0D7F) == base consonant + virama (the model emits
    # atomic, GT uses the sequence). Fold atomic -> sequence so they compare equal.
    CHILLU = {"ൺ": "ണ്", "ൻ": "ന്", "ർ": "ര്", "ൽ": "ല്", "ൾ": "ള്", "ൿ": "ക്"}

    def norm(s):
        s = __import__("unicodedata").normalize("NFC", s).replace("‌", "").replace("‍", "")
        s = s.replace("ેા", "ો").replace("ૈા", "ૌ")
        for a, b in CHILLU.items():
            s = s.replace(a, b)
        return s
    pred, lab = norm(pred), norm(lab)
    exact += pred == lab
    sm = difflib.SequenceMatcher(None, lab, pred)
    den += len(lab)
    num += len(lab) - sum(b.size for b in sm.get_matching_blocks())
    if len(examples) < 8:
        examples.append((lab, pred))

print(f"exact-match: {exact}/{len(lines)} = {exact/len(lines):.1%}   CER ~ {num/max(den,1):.2%}")
for lab, pred in examples:
    print(("OK  " if lab == pred else "X   ") + repr(lab) + "  ->  " + repr(pred))
