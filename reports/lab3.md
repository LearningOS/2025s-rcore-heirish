### 实现总结
- sys_spawn实现:在syscall module里判断传入的文件名有效性， 并读取elf内容。然后在task module的TaskControlBlock里实现最终的spawn， 步骤如下:从传入的elf数据生成一个全新的TaskControlBlock,其中包含的新的process,新的kernel stack以及新的内存空间。然后将新生成的TaskControlBlock与当前的运行的task的TaskcontrolBlock作父子进程的关联。然后将新生成的TaskControlBlock加入注册到TASKMANAGER的待执行队列。最后返回新生成的TaskControlBlock的pid即可
- sys_set_priority:在syscall module里做priority的是否有效的检查。然后在TaskControlBlockInner添加stride以及priority两个属性，syscall_set_priority时更改TaskControlBlockInner的priority属性即可.具体的调度算法实现放在TaskManager的fetch()函数中。使用的是暴力查找具有最小stride的TaskControlBlock,pop这个task并更新其stride。
- refer: stride3調度算法
  ```
  stride 调度算法
  算法描述如下:
  1.为每个进程设置一个当前 stride，表示该进程当前已经运行的“长度”。另外设置其对应的 pass 值（只与进程的优先权有关系），表示对应进程在调度后，stride 需要进行的累加值。
  2.每次需要调度时，从当前 runnable 态的进程中选择 stride 最小的进程调度。对于获得调度的进程 P，将对应的 stride 加上其对应的步长 pass。
  3.一个时间片后，回到上一步骤，重新调度当前 stride 最小的进程。
  
  可以证明，如果令 P.pass = BigStride / P.priority 其中 P.priority 表示进程的优先权（大于 1），而 BigStride 表示一个预先定义的大常数，则该调度方案为每个进程分配的时间将与其优先级成正比。证明过程我们在这里略去，有兴趣的同学可以在网上查找相关资料。
  
  其他实验细节：
  stride 调度要求进程优先级>=2, 所以设定进程优先级<=1 会导致错误。
  进程初始 stride 设置为 0 即可。
  进程初始优先级设置为 16。
  ```
### 问答题
- stride 算法深入
    - stride 算法原理非常简单，但是有一个比较大的问题。例如两个 pass = 10 的进程，使用 8bit 无符号整形储存 stride， p1.stride = 255, p2.stride = 250，在 p2 执行一个时间片后，理论上下一次应该 p1 执行。
    实际情况是轮到 p1 执行吗？为什么？
        > 不是，由于u8溢出，在p2執行完當前時間片后，新的p2.stride = 250 + 10, 溢出后值為4, 所以在下次fetch task時，由于p2.stride < p1.stride, 返回的還會是p2, 所以會執行p2.
    - 我们之前要求进程优先级 >= 2 其实就是为了解决这个问题。可以证明， 在不考虑溢出的情况下 , 在进程优先级全部 >= 2 的情况下，如果严格按照算法执行，那么 STRIDE_MAX – STRIDE_MIN <= BigStride / 2。
    为什么？尝试简单说明（不要求严格证明）。
    - 已知以上结论，考虑溢出的情况下，可以为 Stride 设计特别的比较器，让 BinaryHeap<Stride> 的 pop 方法能返回真正最小的 Stride。补全下列代码中的 partial_cmp 函数，假设两个 Stride 永远不会相等。
        ```
        use core::cmp::Ordering;
        
        struct Stride(u64);
        impl PartialOrd for Stride {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                // ...
            }
        }
        impl PartialEq for Stride {
            fn eq(&self, other: &Self) -> bool {
                false
            }
        }
        ```
        TIPS: 使用 8 bits 存储 stride, BigStride = 255, 则: (125 < 255) == false, (129 < 255) == true.
### 荣誉准则
1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：
   > NA
2. 此外，我也参考了以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：
   > rCore-Tutorial-Guide-2025S文档
3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。
4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。