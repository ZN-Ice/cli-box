/superpowers:systematic-debugging 测试一下，release.sh编译构建后，使用release中的cli打开opencode，然后使用cli输入"你是谁？"，
  并出发回车发送。这个过程每一步都截图分析，保存到@release_test/${{yyyy-mm-dd hh-mm-ss}}文件夹下。

/superpowers:systematic-debugging 使用 @release.sh
  打包编译release，然后你进行一个简单场景的测试，场景一：在沙箱启动claude以后，回车确认，然后输入你是谁？
      然后出发回车发送。场景二：在沙箱启动zsh，然后输入echo "hello
  world"，然后回车发送。每一步操作后都截图保存到release_test/${{时间戳，yyyy
    -mm-dd-hh-mm-ss}}文件夹下，然后检查截图结果，查看是否符合预期，注意读取图片前先判断图片是否存在问题

## 当前存在问题的场景

- /superpowers:systematic-debugging 使用 @release.sh 打包编译release，然后先打开opencode，再打开zsh，会发现两个窗口都是opencode的界面
- 当前打开的claude，在回车确认后，界面上会有选项`Yes, I trust this folder`残留