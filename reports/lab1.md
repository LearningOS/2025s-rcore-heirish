### 实现总结
- 在struct TaskControlBlock添加一个长度为512(目前最大的syscall id为410)的usize数组用于存储当前task的每个syscall调用次数。数组index即为syscall id.
- 增加kernel stack大小: 4096 * 16  -> 4096 * 32.因为在TaskCtrolBlock中添加了syscall调用次数属性后，TaskManager struct的大小会增加MAX_APP_NUM * MAX_SYSCALL_ID * sizeof(usize) = 16 * 512 * 8byte = 16 * 4k = 64k
- 在AppManager中添加两个函数increase_current_syscall_num, get_task_syscall_number分别用于递增，获取当前task的某个系统调用次数，参数都为syscall id.
- 按要求实现sys_trace函数功能，在trace request为2时调用APPManager的get_task_syscall_number
- 在总的syscall分发函数os/src/syscall/mod.rs:syscall中，调用increase_current_syscall_num递增对应syscall的调用次数
### 问答题 
- 1.正确进入 U 态后，程序的特征还应有：使用 S 态特权指令，访问 S 态寄存器后会报错。 请同学们可以自行测试这些内容（运行 三个 bad 测例 (ch2b_bad_*.rs) ）， 描述程序出错行为，同时注意注明你使用的 sbi 及其版本。
  > 直接用的make run查看输出结果，这三个bad测例没有正常退出，而是进入了trap流程，在trap_handler的处理逻辑中直接跳到了下一个app的执行。 rustsbi版本为:0.3.0-alpha.2
- 2.深入理解 trap.S 中两个函数 __alltraps 和 __restore 的作用，并回答如下问题:
  - L40：刚进入 __restore 时，sp 代表了什么值。请指出 __restore 的两种使用情景。
    > 刚进入_restore时，sp代表了user stack。__restore的两种使用情景是:case1: start running app by __restor; case2: back to U after handling trape
  - L43-L48：这几行汇编代码特殊处理了哪些寄存器？这些寄存器的的值对于进入用户态有何意义？请分别解释。
    ```
    ld t0, 32*8(sp)
    ld t1, 33*8(sp)
    ld t2, 2*8(sp)
    csrw sstatus, t0
    csrw sepc, t1
    csrw sscratch, t2
    ```
    > 特殊处理了sstatus, spec,和sscrash寄存器。其中sstatus保存了Trap之前的CPU特权级信息U, 而spec保存了Trap处理完成后会执行的下一条指令。也就是U态的发生trap的指令的下一条，在trap处理完成后会返回到这个地址继续执行。sscratch保存了user stack地址. 三这个寄存器的值对返回到U态继续执行都至关重要: U态+指令地址+User stack
  - L50-L56：为何跳过了 x2 和 x4？
    ```
    ld x1, 1*8(sp)
    ld x3, 3*8(sp)
    .set n, 5
    .rept 27
       LOAD_GP %n
       .set n, n+1
    .endr
    ```
    > x2:2*8(sp)中存的是进入trap之前的sp值，user stack的地址,己经加载到了sscratch寄存器了。 tp(x4) 寄存器application没用到，除非我们手动出于一些特殊用途使用它，否则一般也不会被用到.
  - L60：该指令之后，sp 和 sscratch 中的值分别有什么意义？`csrrw sp, sscratch, sp`
    > 指令执行后，sp重新指向用户栈栈项，sscratch指向内核栈栈顶
  - __restore：中发生状态切换在哪一条指令？为何该指令执行之后会进入用户态？
    > sret指令解释如下
    ```
    sret 会将当前特权级别从 Supervisor 模式（S-Mode）切换回 Supervisor Previous Privilege Mode（SPP） 所记录的特权级别（通常是 User 模式）。
    恢复程序计数器（PC）:指令会从 sepc（Supervisor Exception Program Counter）寄存器中读取地址，并跳转到该地址继续执行程序。sepc 通常由硬件在异常/中断发生时自动保存。
    ``` 
    而在sret执行前,在代码L43-L48(题2.2), sstatus己被设置了U态，spec己被设置成了下一条用户态指令，sp也恢复成了指现用户栈栈顶，所以这条指令后能正常进入用户态并执行。
  - L13：该指令之后，sp 和 sscratch 中的值分别有什么意义？`csrrw sp, sscratch, sp`
    > 指令执行后，sp指向内核栈栈项，sscratch指向用户栈栈顶
  - 从 U 态进入 S 态是哪一条指令发生的？
    > call trap_handler
### 荣誉准则
1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：
   > NA
2. 此外，我也参考了 以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：
   > rCore-Tutorial-Guide-2025S文档
3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。
4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。
