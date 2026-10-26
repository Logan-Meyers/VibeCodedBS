# digits.bf: prints 0123456789 and a newline

++++++[>++++++++<-]     # c1 = 48 ('0')
>>++++++++++            # c2 = 10 (loop counter)
>++++++++++             # c3 = 10 (newline)
<
[
    <.+>-               # print digit; next digit; count down
]
>.
