"""Mini U-Net for per-strip ink coverage: RGB strip in, soft alpha matte out.

Fully convolutional; height is 48 in training. `levels=N` needs (H, W) divisible by
2**N (so levels<=4 at H=48: 48/16=3). Each extra level enlarges the receptive field
so the interiors of thick/superbold strokes (edge-starved at 2 levels) get filled, not
just outlined; depth buys reach far more cheaply (in FLOPs) than wider channels do.
Layer names are kept fixed per level so older 2-/3-level checkpoints still load.
"""

import torch
import torch.nn as nn
import torch.nn.functional as F


def conv_block(cin: int, cout: int) -> nn.Sequential:
    return nn.Sequential(
        nn.Conv2d(cin, cout, 3, padding=1),
        nn.BatchNorm2d(cout),
        nn.ReLU(inplace=True),
        nn.Conv2d(cout, cout, 3, padding=1),
        nn.BatchNorm2d(cout),
        nn.ReLU(inplace=True),
    )


class InkUNet(nn.Module):
    def __init__(self, base: int = 16, levels: int = 2, bold_from: int = 1, detach_bold: bool = False,
                 bold_head: str = "dilated", rule: bool = False, rule_head: str = "dilated",
                 color: bool = False):
        super().__init__()
        self.levels = levels
        self.bold_from = bold_from
        self.detach_bold = detach_bold
        self.bold_head_kind = bold_head
        self.rule = rule
        self.rule_head_kind = rule_head
        self.color = color
        self.enc1 = conv_block(3, base)
        self.enc2 = conv_block(base, base * 2)
        self.enc3 = conv_block(base * 2, base * 4)
        self.pool = nn.MaxPool2d(2)
        if levels >= 3:
            self.enc4 = conv_block(base * 4, base * 8)
            self.up3 = nn.ConvTranspose2d(base * 8, base * 4, 2, stride=2)
            self.dec3 = conv_block(base * 8, base * 4)
        if levels >= 4:
            self.enc5 = conv_block(base * 8, base * 16)
            self.up4 = nn.ConvTranspose2d(base * 16, base * 8, 2, stride=2)
            self.dec4 = conv_block(base * 16, base * 8)
        self.up2 = nn.ConvTranspose2d(base * 4, base * 2, 2, stride=2)
        self.dec2 = conv_block(base * 4, base * 2)
        self.up1 = nn.ConvTranspose2d(base * 2, base, 2, stride=2)
        self.dec1 = conv_block(base * 2, base)
        self.matte_head = nn.Conv2d(base, 1, 1)
        # Bold reads from a deeper decoder stage (bold_from: 1=dec1 full-res/base ch,
        # 2=dec2 ½-res/base·2, 3=dec3 ¼-res/base·4). Deeper = more channels and more
        # spatial pooling to denoise the per-pixel stroke-width estimate; the bold logit
        # is upsampled back to full res (per-region pooling downstream wants no crispness).
        bc = base * (2 ** (bold_from - 1))
        if bold_head == "1x1":
            self.bold_head = nn.Conv2d(bc, 1, 1)
        else:
            self.bold_head = nn.Sequential(
                nn.Conv2d(bc, bc, 3, padding=4, dilation=4),
                nn.ReLU(inplace=True),
                nn.Conv2d(bc, bc, 3, padding=8, dilation=8),
                nn.ReLU(inplace=True),
                nn.Conv2d(bc, 1, 1),
            )
        # Rule head (under/strike/over line): reads full-res d1. A rule is a long, thin,
        # horizontal structure, so the dilated variant's wide receptive field helps it
        # separate a real rule from a glyph's horizontal strokes. Optional — kept absent
        # for matte+bold checkpoints so they still load strict.
        if rule:
            if rule_head == "1x1":
                self.rule_head = nn.Conv2d(base, 1, 1)
            else:
                self.rule_head = nn.Sequential(
                    nn.Conv2d(base, base, 3, padding=4, dilation=4),
                    nn.ReLU(inplace=True),
                    nn.Conv2d(base, base, 3, padding=8, dilation=8),
                    nn.ReLU(inplace=True),
                    nn.Conv2d(base, 1, 1),
                )
        # Colour head (FBA-style joint prediction): 6 logit channels off full-res d1 —
        # F = decontaminated ink RGB, B = background RGB, both sigmoid at the consumer.
        # Predicting F directly (instead of estimating colour post-hoc from matte-selected
        # pixels) is what keeps a matte over-marking failure from poisoning the colour.
        # Optional so matte/bold/rule checkpoints still load strict.
        if color:
            self.color_head = nn.Conv2d(base, 6, 1)

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        e1 = self.enc1(x)
        e2 = self.enc2(self.pool(e1))
        e3 = self.enc3(self.pool(e2))
        if self.levels >= 3:
            e4 = self.enc4(self.pool(e3))
            if self.levels >= 4:
                e5 = self.enc5(self.pool(e4))
                e4 = self.dec4(torch.cat([self.up4(e5), e4], dim=1))
            e3 = self.dec3(torch.cat([self.up3(e4), e3], dim=1))
        d2 = self.dec2(torch.cat([self.up2(e3), e2], dim=1))
        d1 = self.dec1(torch.cat([self.up1(d2), e1], dim=1))
        bold_src = {1: d1, 2: d2, 3: e3}[self.bold_from]
        # detach_bold: bold gradient does not flow into the shared trunk, so the trunk
        # optimises purely for the matte (no negative transfer) and the bold head is a
        # readout on the matte-optimal features.
        if self.detach_bold:
            bold_src = bold_src.detach()
        bold = self.bold_head(bold_src)
        if self.bold_from > 1:
            bold = F.interpolate(bold, size=d1.shape[-2:], mode="bilinear", align_corners=False)
        outs = [self.matte_head(d1), bold]
        if self.rule:
            outs.append(self.rule_head(d1))
        if self.color:
            outs.append(self.color_head(d1))
        return torch.cat(outs, dim=1)


def param_count(model: nn.Module) -> int:
    return sum(p.numel() for p in model.parameters())


if __name__ == "__main__":
    m = InkUNet()
    print(f"params: {param_count(m):,}")
    y = m(torch.zeros(1, 3, 48, 320))
    print("out:", tuple(y.shape))
