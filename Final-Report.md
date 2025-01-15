<center>
<h1>CS120 Project Report</h1>
Hanrui Zhang (张涵锐), Xiang Li (李想)
</center>

### Introduction
**For those starting from the summer vacation: start from PA2**

CS120 project, the Athernet, requires students to implement the physical layer in PA1, the mac layer in PA2, the ip layer in PA3 and PA4, and the application layer if needed in PA3 and PA4. We strongly recommend students who try to work for it from the summer vacation starting from PA2, where you can simulate your physical layer transmission on file streams. If not, you may lose much time on invalid debugging attempts, since PA1 highly depends on your audio device.

### PA1 - Acoustic Link
We started our project from the summer vacation. We believed that the more we've done in the summer, the less we needed to hurry during the semester. However, in the athernet project, 2 devices are required at least to test for trasmission, which is hard for 2 students apart to check the functions. Although we redirected the output to `.wav` file, which is then used to check the wave form, and used smartphone's speaker to test for receiving, we still spent much time for adjustment after the semester has began.

#### Audio Output
Before we started our project, Linshu Yang has offered his [Rust ASIO Guide](https://acm.shanghaitech.edu.cn/rust-asio/00_introduction.html), who has highly recommended us to code in Rust. This guide helped us a lot, except for the `OutputAudioStream`, which bothered us in PA2 later.

For the preamble wave, we worked on it for lots of days. During summer vacation, we simply thought any regular signal can be used as preamble. Therefore, 1010101010 encoded by carrier wave's pattern made us confusing and at a loss. At first we considered the detect programme leaded to this situation and changed 3 versions of implementation. However nothing was changed. This moment we realized that the choice of preable might be the prime problem. We started to find reference solution and discovered the chirp, which can work much better in current situation and one of our biggest obstacle was cleared. We could receive correct data sometimes.

For the carrier wave, we chose PSK of sine wave for the representation of `0` and `1`. However, as the testing work went on, we found that our audio device can only behave well in specific frequencies, which prevented us to achieve the speed requirement using a single frequency carrier. That brought out the OFDM.

To achieve orthogonality, classic OFDM requires the quotient between each carrier frequencies to be a power of 2. According to our calculation, to reach the highest trasmission efficiency, the number of carriers must be 1 or 2 when the highest frequency keeps the same. For the latter, the introduction of the lower frequency means more repetition for a single symbol in higher frequency, which enhances the stability. So we chose the classic OFDM with 2 orthogonal carriers.

#### Device
Device does matter. At first we start from PC microphone and iPhone speaker and the result wasn't perfect but stable. But TA told us the phone's speaker is banned and we must use two PC. Then we tried to use two PC directly and the result was extremely awful. Then we bought a microphone, use the new microphone with another PC's speaker. It was dispiriting that no improvement was achieved and we started to find the problem of programme again for couple of days without any progress. When we were at a loss, Lei Huang told us that the speaker also affected the result greatly and there was a woofer we could borrow. After the update of equipment, we could achieve 80% correctness however it's far from requirements. Because the switch of device could achive great change, we borrowed another speaker and all test passed.  We still remember the day when we just changed a speaker and suddenly all testcases were passed very well.

#### Framing
As 100% correctness is required for full score of some mandatory tasks, we imported Reed-Solomon ECC to reduce transmission errors. Since the library requires encoding for specified length of bytes, we made our physical frame to fit in these APIs. And as ECC is employed, we didn't implement CRC on our physical frame. 

### PA2 - Manage Multiple Access
The physical transmission for PA2 is somehow different from one for PA1, as the output stream in [Rust ASIO Guide](https://acm.shanghaitech.edu.cn/rust-asio/00_introduction.html) behaves different in single transmission and multiple transmissions as we need to send a single mac frame at once.

#### Physical Transmission
In [Rust ASIO Guide](https://acm.shanghaitech.edu.cn/rust-asio/00_introduction.html), the `rodio::stream::OutputStream` is created for multiple time use. However, it turns out that the receiver cannot decode the acoustic signal correctly after the first transmission. We spent much time debugging, to locate the issue. The solution was that we created the `rodio::stream::OutputStream` every time we transmitted the mac frame, and destoried it after transmission.

Since cable transmission is more reliable than the air, we removed the ECC part, and added 1-byte checksum before the whole physical frame. Then, we used Manchester Coding for symbol representation, which only used 2 sample points, bringing more transmission speed.

#### Mac Frame
To spent less time in tranferring non-payload data, we compressed the mac header into 2 bytes: 4 bits for destination mac address, 4 bits for source mac address, 6 bits for frame id, and 2 bits for frame type, after which the payload came.

#### CSMA
CSMA wasn't a problem for us, since we just needed to design an agreed-mechanism to avoid the collision from both the nodes. Instead, CSMA with Interference brought much trouble for us, since the collision from jammer is somehow unpredictable. We have referred to the parameters in Linshu Yang and Lei Huang's project, but it didn't work for our implementation. We also spent a whole afternoon adjusting our configuration during the code check, but still failed passing task 4.

### PA3 & PA4 - To the Internet & Above IP
To pass some specific tasks, we chose Wintun as our virtual network device at the beginning, which made PA3 and PA4 somehow similar.

#### Virtual Network Device
As wintun offered APIs to process IP packets from the application layer, we can just check the content, and develop a firewall-liked project. We call the packets from the upper layer and sending to the mac layer the "down" packet, and the opposite be the "up" packet.

When the gateway is set to the network device, the OS will send most of the packets to it, and thus a filter is necessary:
- In PA3, we only allow ICMP packets send to the mac layer. For ICMP request, we generate the reply packet in our program to improve the performance.
- In PA4, besides ICMP, DNS for websites in the tasks, and packets corresponding to the ip of these websites were also allowed.

Task 2 in PA4 required us to specify the sequence number of our TCP connection. As it would spend much time to do so on our project, which required to detect the TCP flags, change the sequence number of the down packet (also record it), set the sequence number of the up packet to the original one, (and Hanrui was busy reviewing for other courses, also determined to be back home early), we implemented a simpler one: added a fixed number to the down packet, and subtracted it to the up packet. Of course checksum in TCP header was still needed to modify.

#### Router
As our project only worked as a firewall, we needed to modify the route table on Windows to achieve Internet tasks.

Windows can forwarding IP packets whose destination is not itself, but can't handle correctly when one interface receive another interface's packet. To cope with it, we use `New-NetNat` to create a Nat Device (on Acoustic) and enable all device's forwarding function by `Set-NetIPInterface -Forwarding Enable`. Then Acoustic works well and we can access outer web. However it dosen't work on the forwarding between hotspot and Acoustic. We guess the problem comes from the device share between hotspot and ethernet which narrow the forwarded interface. Because we don't need to implement the Nat and accessing outer web simultaneously, we just replace the share device with Acoustic. Then hotspot and Acoustic can connect to each other. Here we can meet all demand in normal part of PA3 and PA4.