import torch
from datasets import load_dataset
from transformers import DataCollatorForSeq2Seq, Trainer, TrainingArguments
from unsloth import FastModel
from unsloth.chat_templates import get_chat_template

MAXSEQ = 1024

model, tok = FastModel.from_pretrained(
    model_name="pkupie/gemma-3-4b-ug-cpt",
    max_seq_length=MAXSEQ,
    load_in_4bit=True,
    full_finetuning=False,
)
model = FastModel.get_peft_model(
    model,
    finetune_vision_layers=False,
    finetune_language_layers=True,
    finetune_attention_modules=True,
    finetune_mlp_modules=True,
    r=32, lora_alpha=64, lora_dropout=0.0, bias="none", random_state=13,
)
tok = get_chat_template(tok, chat_template="gemma-3")
tokenizer = getattr(tok, "tokenizer", tok)  # multimodal Gemma3Processor -> underlying text tokenizer


def tokenize(ex):
    full = tok.apply_chat_template(ex["messages"], tokenize=False, add_generation_prompt=False)
    ids = tokenizer(full, truncation=True, max_length=MAXSEQ, add_special_tokens=False)["input_ids"]
    prompt = tok.apply_chat_template(ex["messages"][:1], tokenize=False, add_generation_prompt=True)
    p_ids = tokenizer(prompt, add_special_tokens=False)["input_ids"]
    n = min(len(p_ids), len(ids))
    labels = [-100] * n + ids[n:]
    return {"input_ids": ids, "attention_mask": [1] * len(ids), "labels": labels}


train = load_dataset("json", data_files="gemma.train.jsonl", split="train").map(tokenize, remove_columns=["messages"])
valid = load_dataset("json", data_files="gemma.valid.jsonl", split="train").map(tokenize, remove_columns=["messages"])

collator = DataCollatorForSeq2Seq(tokenizer, padding=True, label_pad_token_id=-100)

args = TrainingArguments(
    per_device_train_batch_size=8,
    gradient_accumulation_steps=2,
    warmup_steps=10,
    num_train_epochs=3,
    learning_rate=2e-4,
    logging_steps=10,
    eval_strategy="steps",
    eval_steps=40,
    save_strategy="no",
    optim="adamw_8bit",
    weight_decay=0.01,
    lr_scheduler_type="linear",
    seed=13,
    output_dir="out",
    report_to="none",
    bf16=True,
)

trainer = Trainer(
    model=model,
    args=args,
    train_dataset=train,
    eval_dataset=valid,
    data_collator=collator,
    processing_class=tokenizer,
)
trainer.train()
model.save_pretrained_merged("merged_16bit", tok, save_method="merged_16bit")
print("SAVED merged_16bit", flush=True)
