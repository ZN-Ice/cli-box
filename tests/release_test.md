使用 @README.md 中的cli命令完成下面场景的验证：
1. /superpowers:systematic-debugging 测试一下，release.sh编译构建后，使用release中的cli打开opencode，然后使用cli输入"你是谁？"， 并出发回车发送。这个过程每一步都截图分析，保存到@release_test/${{yyyy-mm-dd hh-mm-ss}}文件夹下。
2. /superpowers:systematic-debugging 使用 @release.sh 打包编译release，然后你进行一个简单场景的测试，场景一：在沙箱启动claude以后，回车确认，然后输入你是谁？然后出发回车发送。场景二：在沙箱启动zsh，然后输入echo "hello world"，然后回车发送。每一步操作后都截图保存到release_test/${{时间戳，yyyy-mm-dd-hh-mm-ss}}文件夹下，然后检查截图结果，查看是否符合预期，注意读取图片前先判断图片是否存在问题
3. /superpowers:systematic-debugging 使用 @release.sh 打包编译release，然后先打开opencode，再打开zsh，分别CLI命令行截图两个窗口，获取到的是各自的界面
4. 当前打开的claude，在回车确认后，判断界面上，不会有选项`Yes, I trust this folder`残留
5. 新增测试点
  - pnpm dev 后 Electron 窗口正常打开
  - 终端日志显示 "Daemon started on port XXXX"
  - sandbox start zsh 创建新 Tab，xterm.js 显示 zsh 提示符
  - sandbox start claude 创建另一个 Tab，显示 Claude Code
  - Tab 切换正常（离屏定位策略）
  - 截图功能正常
  - 关闭一个 Tab 不影响其他 Tab
6. `sandbox start opencode`打开opencode，分别采用默认的截图和带窗口的截图，截取该opencode的运行状态，查看图片，是否符合预期的设计
7. 在release_test/${{时间戳，yyyy-mm-dd-hh-mm-ss}}文件夹下，生成markdown的最终测试报告