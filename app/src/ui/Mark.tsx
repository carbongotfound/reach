// Reach's own logo at sidebar size: the pointing hand and dot, one colour, no tile. #111 on light, white on dark,
// following Reach's Theme setting (System follows Windows). assets/brand/logo.py bare; curled fingers closed at this size.
import mark from "../assets/brand/reach-mark.svg";
import markDark from "../assets/brand/reach-mark-dark.svg";

export const Mark = ({ size = 22 }: { size?: number }) => (
  <span className="mark" style={{ width: size, height: size }} aria-hidden>
    <img className="on-light" src={mark} width={size} height={size} alt="" draggable={false} />
    <img className="on-dark" src={markDark} width={size} height={size} alt="" draggable={false} />
  </span>
);
