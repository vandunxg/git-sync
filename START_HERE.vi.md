# Bắt đầu triển khai Git Backup Manager

## 1. Bộ này dùng để làm gì?

Đây là bộ **instruction + RULE + spec + plan** cho AI agents,
không phải mã nguồn ứng dụng đã hoàn thành.
Agent phải làm từng task, chạy test, gắn bằng chứng và
đối chiếu UI trước khi báo xong. Phần spec kỹ thuật
viết bằng English; hướng dẫn này bằng tiếng Việt.

## 2. Cách đưa vào project

Giải nén và để thư mục này tại root repo mới.
Với repo có sẵn, để dưới `blueprint/` rồi yêu cầu agent
đọc `blueprint/AGENTS.md`; không ghi đè rule cũ khi chưa kiểm tra.
Không đặt token GitHub/GitLab vào prompt, file spec hay Git.

Chạy kiểm tra bộ tài liệu:

```bash
python3 scripts/lint_pack.py
python3 -m unittest discover -s scripts/tests -v
```

## 3. Prompt để bắt đầu

Copy prompt sau cho agent trong repo:

```text
Read AGENTS.md, docs/spec/SOURCE_OF_TRUTH.md and agents/EXECUTION_PROTOCOL.md.
Start with phase P00. Inspect the actual repository and environment first.
Execute one ready task at a time from docs/plan/TASKS.json.
Do not invent APIs, features, tests, UI states or completion evidence.
Do not change approved scope or visual baselines to make your output pass.
Use rules for Rust, Frontend, security, Git safety and visual acceptance.
Write task evidence and update the task state only after checks are executed.
Stop only the affected task when required evidence, access or approval is missing.
Report in Vietnamese: changes, commands run, results, blockers and next ready task.
```

Khi tiếp tục phiên mới, dùng [resume prompt](agents/prompts/RESUME.md).
Không bảo agent làm toàn bộ project trong một lần và tự
đánh dấu xong khi chưa có test.

## 4. Khóa UI để tránh lệch expectation

Bản tham chiếu là Figma **Refined V2**, không phải bộ ảnh dark
ban đầu hay page Clean UI. Các dot trạng thái phải được
thay bằng semantic icon theo [icon registry](contracts/icons.json).

Lần đọc Figma trong phiên đóng gói này bị chặn quota.
Vì vậy chưa có ảnh baseline được duyệt đính kèm.
Agent phải lấy ảnh export/metadata khi có quyền truy cập, hoặc
nhận ảnh từ bạn, rồi ghi approval và SHA-256 trong
`docs/ui/BASELINES.json`. Không được tự vẽ ảnh khác rồi
tự coi là bản bạn đã duyệt.

```bash
python3 scripts/check_visual_baselines.py
```

Lệnh này hiện phải trả `BLOCKED`, không phải `PASS` giả.
Backend và các task không phụ thuộc visual approval vẫn có thể làm.

## 5. Cách nghiệm thu

Mỗi task cần requirement ID, file diff, test ID, lệnh đã chạy và
kết quả. Task UI cần ảnh implementation, overlay/diff với baseline,
kiểm tra chồng chéo, keyboard và native Tauri khi liên quan.
Không chỉ kiểm tra phần tử nằm trong khung ngoài.

Sau khi scaffold thật đã có, chạy:

```bash
python3 scripts/run_gates.py --profile core --repo-root /path/to/app --evidence-dir /path/to/evidence/core
python3 scripts/run_gates.py --profile frontend --repo-root /path/to/app --evidence-dir /path/to/evidence/frontend
```

Các gate này không thể PASS khi chưa có source/toolchain.
Blueprint không thể bảo đảm agent tuân thủ chỉ bằng câu chữ;
CI, evidence và review là các chốt kiểm soát bắt buộc.
