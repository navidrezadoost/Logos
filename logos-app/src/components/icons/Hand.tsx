import React from "react";
import type { SVGProps } from "react";

export interface HandProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const Hand = React.forwardRef<SVGSVGElement, HandProps>(
  ({ size, className, style, ...props }, ref) => {
    const dimensions = size ? { width: size, height: size } : {
      width: props.width || 24,
      height: props.height || 24
    };

    return (
      <svg
        ref={ref}
        viewBox="0 0 640 640"
        xmlns="http://www.w3.org/2000/svg"
        width={dimensions.width}
        height={dimensions.height}
        fill={props.fill || "currentColor"}
        className={className}
        style={style}
        {...props}
      >
        <path fill="currentColor" d="M352.2 96L352.2 64L288.2 64L288.2 320L256.2 320L256.2 96L192.2 96L192.2 400C192.2 401.5 192.2 403.1 192.3 404.6C160.8 374.6 136.6 351.6 119.7 335.5L64.5 393.4C72.7 401.2 114.2 440.8 189 512C232.1 553.1 289.4 576 349 576L368.2 576C465.4 576 544.2 497.2 544.2 400L544.2 160L480.2 160L480.2 320L448.2 320L448.2 96L384.2 96L384.2 320L352.2 320L352.2 96z"/>
      </svg>
    );
  }
);

Hand.displayName = "Hand";

export default Hand;
